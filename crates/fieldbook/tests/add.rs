//! Integration tests for `fieldbook add` (`add-new-and-edition`).

#![expect(
    clippy::disallowed_methods,
    reason = "integration tests prepare temporary fixtures and inspect written files and journal state"
)]

mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fieldbook::World;
use fieldbook_core::add::{
    AddError, AddOptions, AddOutcome, CompletedSourceCells, RESERVED_SOURCE_FIELDS, SourceRow,
    plan_add, render_source_row,
};
use fieldbook_core::audit::{Principal, Sha256};
use fieldbook_core::check::{self, Host};
use fieldbook_core::ids::SourceId;
use fieldbook_core::schema::Register;
use fieldbook_core::schema::table::read as read_csv;
use serde_json::Value;

use support::faulty::{Fault, FaultKind, FaultyWorld};
use support::temp_copy;
use support::world::TestWorld;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini")
}

fn svec(args: &[&str]) -> Vec<String> {
    args.iter().map(|s| (*s).to_owned()).collect()
}

fn write_input(dir: &Path, name: &str, content: &[u8]) -> PathBuf {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("create input parent: {e}"));
    }
    std::fs::write(&path, content).unwrap_or_else(|e| panic!("write input file: {e}"));
    path
}

fn audit_show_json(world: &dyn World, extra: &[&str]) -> Value {
    let mut args = vec!["audit", "show", "--json"];
    args.extend_from_slice(extra);
    let out = fieldbook::run(&svec(&args), world);
    assert_eq!(out.code, 0, "audit show failed: {}", out.stderr);
    serde_json::from_str(&out.stdout).unwrap_or_else(|e| panic!("parse audit show json: {e}"))
}

#[test]
fn adds_a_new_source() {
    let tmp = temp_copy(&fixture_root());
    write_input(
        tmp.path(),
        "synthetic.txt",
        b"Synthetic text, made up for tests.\n",
    );
    let world = TestWorld::new(tmp.path());

    let out = fieldbook::run(&svec(&["add", "synthetic.txt", "--confirm-public"]), &world);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "added S-0004-v1 synthetic.txt\n");

    let stored = tmp.path().join("corpus/sources/public/S-0004-v1.txt");
    assert!(stored.is_file(), "stored original must exist");

    let sources_bytes =
        std::fs::read(tmp.path().join("corpus/registers/sources.csv")).expect("read sources.csv");
    let table = read_csv(&sources_bytes).expect("parse sources.csv");
    assert_eq!(table.rows.len(), 5);
    let last = table.rows.last().expect("new row");
    assert_eq!(last.cells[0], "S-0004");
    assert_eq!(last.cells[1], "1");
    assert_eq!(last.cells[11], "synthetic.txt");
    assert_eq!(last.cells[16], world.today().to_string());
    assert_eq!(last.cells[17], "active");
    assert_eq!(last.cells[20], "public");
    assert!(last.cells[23].is_empty());
    assert!(last.cells[24].is_empty());
    assert!(last.cells[25].is_empty());
}

#[test]
fn adds_a_version_with_source() {
    let tmp = temp_copy(&fixture_root());
    write_input(
        tmp.path(),
        "edition2.txt",
        b"Second edition synthetic text.\n",
    );
    let world = TestWorld::new(tmp.path());

    let out = fieldbook::run(
        &svec(&[
            "add",
            "edition2.txt",
            "--source",
            "S-0001",
            "--confirm-public",
        ]),
        &world,
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "added S-0001-v2 edition2.txt\n");

    let stored = tmp.path().join("corpus/sources/public/S-0001-v2.txt");
    assert!(stored.is_file());
}

#[test]
fn metadata_only_copies_nothing() {
    let tmp = temp_copy(&fixture_root());
    write_input(
        tmp.path(),
        "large.pdf",
        b"Synthetic metadata-only payload.\n",
    );
    let world = TestWorld::new(tmp.path());

    let out = fieldbook::run(
        &svec(&["add", "large.pdf", "--metadata-only", "--confirm-public"]),
        &world,
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "added S-0004-v1 large.pdf\n");

    let stored = tmp.path().join("corpus/sources/public/S-0004-v1.pdf");
    assert!(
        !stored.exists(),
        "--metadata-only must not copy original into corpus/sources/public/"
    );

    let sources_bytes =
        std::fs::read(tmp.path().join("corpus/registers/sources.csv")).expect("read sources.csv");
    let table = read_csv(&sources_bytes).expect("parse sources.csv");
    let last = table.rows.last().expect("new row");
    assert_eq!(last.cells[11], "metadata-only: large.pdf");
}

#[test]
fn second_add_changes_nothing() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "note.txt", b"Idempotent synthetic source.\n");
    let world = TestWorld::new(tmp.path());

    let first = fieldbook::run(&svec(&["add", "note.txt", "--confirm-public"]), &world);
    assert_eq!(first.code, 0);
    assert_eq!(first.stdout, "added S-0004-v1 note.txt\n");

    let tree_after_first = world.read_tree().expect("read tree after first add");
    let head_after_first = audit_show_json(&world, &["--head"]);

    let second = fieldbook::run(&svec(&["add", "note.txt", "--confirm-public"]), &world);
    assert_eq!(second.code, 0);
    assert_eq!(second.stdout, "present S-0004-v1 note.txt\n");

    let tree_after_second = world.read_tree().expect("read tree after second add");
    let head_after_second = audit_show_json(&world, &["--head"]);
    assert_eq!(tree_after_first, tree_after_second);
    assert_eq!(head_after_first["seq"], head_after_second["seq"]);
}

#[test]
fn copy_is_byte_exact() {
    let tmp = temp_copy(&fixture_root());
    let payload: Vec<u8> = (0u8..=127u8)
        .chain(b"\r\n\texact bytes\n".iter().copied())
        .collect();
    write_input(tmp.path(), "Blob.BIN", &payload);
    let world = TestWorld::new(tmp.path());

    let out = fieldbook::run(&svec(&["add", "Blob.BIN", "--confirm-public"]), &world);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "added S-0004-v1 Blob.BIN\n");

    let stored = std::fs::read(tmp.path().join("corpus/sources/public/S-0004-v1.bin"))
        .expect("read stored original");
    assert_eq!(stored, payload);
}

#[test]
fn marking_banner_is_refused_and_tree_unchanged() {
    let tmp = temp_copy(&fixture_root());
    write_input(
        tmp.path(),
        "marked.txt",
        b"CONTROLLED UNCLASSIFIED INFORMATION\nSynthetic body.\n",
    );
    let world = TestWorld::new(tmp.path());
    let before = world.read_tree().expect("tree before");

    let out = fieldbook::run(&svec(&["add", "marked.txt"]), &world);
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "refused marked.txt\n");
    assert!(out.stderr.contains("FB0400"), "stderr: {}", out.stderr);

    let after = world.read_tree().expect("tree after");
    assert_eq!(before, after);
}

#[test]
fn add_without_confirmation_is_refused_fb0410() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "plain.txt", b"Plain synthetic text.\n");
    let world = TestWorld::new(tmp.path());
    let before = world.read_tree().expect("tree before");

    let out = fieldbook::run(&svec(&["add", "plain.txt"]), &world);
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "refused plain.txt\n");
    assert!(out.stderr.contains("FB0410"), "stderr: {}", out.stderr);

    let after = world.read_tree().expect("tree after");
    assert_eq!(before, after);

    let refuse_json = audit_show_json(&world, &["--kind", "refuse"]);
    let events = refuse_json["events"].as_array().expect("events array");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["code"], "FB0410");
    assert_eq!(events[0]["reason"], "unlabelled");
    assert_eq!(events[0]["labels"], serde_json::json!(["unlabelled"]));

    let begin_json = audit_show_json(&world, &["--kind", "write.begin"]);
    assert_eq!(
        begin_json["events"].as_array().expect("events").len(),
        0,
        "refused add must not emit write.begin"
    );
}

#[test]
fn add_with_confirmation_is_public() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "confirmed.txt", b"Confirmed public text.\n");
    let world = TestWorld::new(tmp.path());
    let custom_principal = Principal::parse("os:local\\alice").expect("valid principal");
    world.set_principal(custom_principal);

    let out = fieldbook::run(&svec(&["add", "confirmed.txt", "--confirm-public"]), &world);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "added S-0004-v1 confirmed.txt\n");

    let sources_bytes =
        std::fs::read(tmp.path().join("corpus/registers/sources.csv")).expect("read sources.csv");
    let table = read_csv(&sources_bytes).expect("parse sources.csv");
    let last = table.rows.last().expect("new row");
    assert_eq!(last.cells[20], "public");

    let begin_json = audit_show_json(&world, &["--kind", "write.begin"]);
    let events = begin_json["events"].as_array().expect("events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["labels"], serde_json::json!(["public"]));
    assert_eq!(events[0]["actor"], "os:local\\alice");
}

#[test]
fn add_marked_text_is_refused_even_with_confirmation() {
    let tmp = temp_copy(&fixture_root());
    write_input(
        tmp.path(),
        "marked.txt",
        b"PROPRIETARY AND CONFIDENTIAL\nSynthetic body.\n",
    );
    let world = TestWorld::new(tmp.path());
    let before = world.read_tree().expect("tree before");

    let out = fieldbook::run(&svec(&["add", "marked.txt", "--confirm-public"]), &world);
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "refused marked.txt\n");
    assert!(out.stderr.contains("FB0400"), "stderr: {}", out.stderr);

    let after = world.read_tree().expect("tree after");
    assert_eq!(before, after);
}

#[test]
fn unknown_source_is_a_usage_error() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "note.txt", b"Synthetic text.\n");
    let world = TestWorld::new(tmp.path());
    let before = world.read_tree().expect("tree before");

    let out = fieldbook::run(
        &svec(&["add", "note.txt", "--source", "S-0999", "--confirm-public"]),
        &world,
    );
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("S-0999"), "stderr: {}", out.stderr);
    assert_eq!(before, world.read_tree().expect("tree after"));
}

#[test]
fn add_recovers_after_register_write_before_end() {
    let tmp = temp_copy(&fixture_root());
    let content = b"Synthetic text for post-register recovery.\n";
    write_input(tmp.path(), "recover.txt", content);

    // Pre-compute the exact planned sources.csv bytes so we can simulate a crash right after
    // sources.csv was atomically written on disk and before Journal::end ran.
    let world_plan = TestWorld::new(tmp.path());
    let tree_initial = world_plan.read_tree().expect("initial tree");
    let host = Host::new(tree_initial, world_plan.today());
    let (model, _) = check::load(&host, &check::Options::new(false));
    let content_sha = Sha256::digest(content);
    let AddOutcome::Planned(plan) = plan_add(
        &model,
        "recover.txt",
        content_sha.as_str(),
        &AddOptions::default(),
        None,
        world_plan.today(),
    )
    .expect("plan_add") else {
        panic!("expected Planned");
    };

    // Run 1 with FaultyWorld failing on sources.csv: writes pending.toml, write.begin, and the
    // original file S-0003-v1.txt, then stops before Journal::end.
    let faulty = FaultyWorld::new(
        TestWorld::new(tmp.path()),
        Fault::FailOnName {
            name: "sources.csv",
            kind: FaultKind::Io,
        },
    );
    let interrupted = fieldbook::run(&svec(&["add", "recover.txt", "--confirm-public"]), &faulty);
    assert_eq!(interrupted.code, 3);

    // Now write the exact planned sources.csv to disk to simulate crashing right after the
    // register write completed and before write.end was appended.
    let reg_path = tmp.path().join("corpus/registers/sources.csv");
    std::fs::write(&reg_path, &plan.sources_csv).expect("write planned sources.csv");

    let world = TestWorld::new(tmp.path());

    // 1. Ordinary readers refuse the pending generation before recovery.
    assert!(
        world.read_tree().is_err(),
        "ordinary reader must refuse pending generation before recovery"
    );
    let check_before = fieldbook::run(&svec(&["check"]), &world);
    assert_eq!(check_before.code, 3);

    // 2. A different intent (e.g. --metadata-only) is refused while pending.toml is active.
    let diff_intent = fieldbook::run(
        &svec(&["add", "recover.txt", "--metadata-only", "--confirm-public"]),
        &world,
    );
    assert_eq!(diff_intent.code, 3);

    // 3. A third hash on disk is refused during recovery.
    let mut tampered = plan.sources_csv.clone();
    tampered.push(b'\n');
    std::fs::write(&reg_path, &tampered).expect("write tampered sources.csv");
    let third_hash_out = fieldbook::run(&svec(&["add", "recover.txt", "--confirm-public"]), &world);
    assert_eq!(third_hash_out.code, 3);
    std::fs::write(&reg_path, &plan.sources_csv).expect("restore planned sources.csv");

    // 4. Matching rerun completes the transaction without adding a second write.begin.
    let recovered = fieldbook::run(&svec(&["add", "recover.txt", "--confirm-public"]), &world);
    assert_eq!(recovered.code, 0, "stderr: {}", recovered.stderr);
    assert_eq!(recovered.stdout, "added S-0004-v1 recover.txt\n");
    assert!(!tmp.path().join(".fieldbook/pending.toml").exists());

    // 5. Ordinary readers succeed after recovery.
    assert!(
        world.read_tree().is_ok(),
        "ordinary reader must succeed after recovery"
    );

    // 6. Subsequent rerun reports Present and changes nothing.
    let head_before_rerun = audit_show_json(&world, &["--head"]);
    let rerun = fieldbook::run(&svec(&["add", "recover.txt", "--confirm-public"]), &world);
    assert_eq!(rerun.code, 0);
    assert_eq!(rerun.stdout, "present S-0004-v1 recover.txt\n");
    let head_after_rerun = audit_show_json(&world, &["--head"]);
    assert_eq!(head_before_rerun["seq"], head_after_rerun["seq"]);

    // 7. Both write.begin and write.end carry S-0004 and S-0004-v1, and only one pair exists.
    for id in ["S-0004", "S-0004-v1"] {
        let by_id = audit_show_json(&world, &["--id", id]);
        let events = by_id["events"].as_array().expect("events");
        let kinds: Vec<&str> = events.iter().filter_map(|e| e["kind"].as_str()).collect();
        assert_eq!(
            kinds,
            vec!["write.begin", "write.end"],
            "audit show --id {id} must find both write.begin and write.end"
        );
    }
}

// control: ED-03
#[test]
fn rerun_after_interruption_completes() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "note.txt", b"Interrupted synthetic original.\n");

    let faulty = FaultyWorld::new(
        TestWorld::new(tmp.path()),
        Fault::FailOnName {
            name: "sources.csv",
            kind: FaultKind::Io,
        },
    );
    let first = fieldbook::run(&svec(&["add", "note.txt", "--confirm-public"]), &faulty);
    assert_eq!(first.code, 3);

    // Original was written first; sources.csv still has 4 rows.
    let orig_path = tmp.path().join("corpus/sources/public/S-0004-v1.txt");
    assert!(orig_path.is_file(), "original must be written before row");
    let table_mid = read_csv(
        &std::fs::read(tmp.path().join("corpus/registers/sources.csv")).expect("read sources.csv"),
    )
    .expect("parse sources.csv");
    assert_eq!(table_mid.rows.len(), 4);

    // Rerun with working World finishes the row and writes no second file.
    let world = TestWorld::new(tmp.path());
    let second = fieldbook::run(&svec(&["add", "note.txt", "--confirm-public"]), &world);
    assert_eq!(second.code, 0, "stderr: {}", second.stderr);
    assert_eq!(second.stdout, "added S-0004-v1 note.txt\n");

    let table_after = read_csv(
        &std::fs::read(tmp.path().join("corpus/registers/sources.csv")).expect("read sources.csv"),
    )
    .expect("parse sources.csv");
    assert_eq!(table_after.rows.len(), 5);
    assert!(
        !tmp.path()
            .join("corpus/sources/public/S-0005-v1.txt")
            .exists()
    );
}

// control: AL-01
#[test]
fn io_writes_only_through_atomic() {
    let io_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/io");
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&io_dir)
        .expect("read src/io")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("rs"))
        .collect();
    entries.sort();
    assert!(!entries.is_empty());

    let forbidden = [
        "fs::rename(",
        "fs::write(",
        "fs::create_dir(",
        "fs::create_dir_all(",
        "File::create(",
        "OpenOptions::new()",
    ];

    for path in entries {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if matches!(name, "atomic.rs" | "journal.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read io source file");
        for pat in forbidden {
            assert!(
                !text.contains(pat),
                "raw filesystem call `{pat}` found in {name} outside atomic.rs and journal.rs"
            );
        }
    }
}

// control: AL-01
#[test]
fn add_writes_begin_then_end() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "note.txt", b"Journaled synthetic source.\n");
    let world = TestWorld::new(tmp.path());

    let out = fieldbook::run(&svec(&["add", "note.txt", "--confirm-public"]), &world);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);

    let shown = audit_show_json(&world, &[]);
    let events = shown["events"].as_array().expect("events");
    let kinds: Vec<&str> = events.iter().filter_map(|e| e["kind"].as_str()).collect();
    let begin_pos = kinds
        .iter()
        .position(|&k| k == "write.begin")
        .expect("write.begin present");
    let end_pos = kinds
        .iter()
        .position(|&k| k == "write.end")
        .expect("write.end present");
    assert_eq!(end_pos, begin_pos + 1);
    assert_eq!(events[end_pos]["outcome"], "ok");
}

#[test]
fn refused_add_is_journaled() {
    let tmp = temp_copy(&fixture_root());
    write_input(
        tmp.path(),
        "secret.txt",
        b"EXPORT CONTROLLED - ITAR\nSynthetic body.\n",
    );
    let world = TestWorld::new(tmp.path());

    let out = fieldbook::run(&svec(&["add", "secret.txt", "--confirm-public"]), &world);
    assert_eq!(out.code, 1);

    let refuse_json = audit_show_json(&world, &["--kind", "refuse"]);
    let events = refuse_json["events"].as_array().expect("events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["code"], "FB0400");
    assert_eq!(events[0]["reason"], "store");
    assert_eq!(events[0]["verb"], "add");
}

#[test]
fn refused_add_names_store_and_labels() {
    let tmp = temp_copy(&fixture_root());
    let content = b"CONTROLLED UNCLASSIFIED INFORMATION\nSynthetic body.\n";
    write_input(tmp.path(), "fouo.txt", content);
    let world = TestWorld::new(tmp.path());

    let out = fieldbook::run(&svec(&["add", "fouo.txt", "--confirm-public"]), &world);
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("corpus"), "stderr: {}", out.stderr);
    assert!(out.stderr.contains("marked"), "stderr: {}", out.stderr);

    let refuse_json = audit_show_json(&world, &["--kind", "refuse"]);
    let events = refuse_json["events"].as_array().expect("events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["store"], "corpus");
    assert_eq!(events[0]["labels"], serde_json::json!(["marked"]));
    assert_eq!(events[0]["sha256"], Sha256::digest(content).as_str());
    assert_eq!(
        events[0]["name_sha256"],
        Sha256::digest(b"fouo.txt").as_str()
    );
    let raw_json = serde_json::to_string(&events[0]).expect("serialize event");
    assert!(
        !raw_json.contains("CONTROLLED UNCLASSIFIED INFORMATION"),
        "refuse event must never hold matched banner text"
    );
}

#[test]
fn interrupted_add_is_visible_then_completed() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "partial.txt", b"Partial add synthetic text.\n");

    let faulty = FaultyWorld::new(
        TestWorld::new(tmp.path()),
        Fault::FailOnName {
            name: "sources.csv",
            kind: FaultKind::Io,
        },
    );
    let first = fieldbook::run(&svec(&["add", "partial.txt", "--confirm-public"]), &faulty);
    assert_eq!(first.code, 3);

    let world = TestWorld::new(tmp.path());

    // Ordinary readers refuse the pending generation until recovery.
    assert!(world.read_tree().is_err());
    let check_out = fieldbook::run(&svec(&["check"]), &world);
    assert_eq!(check_out.code, 3);

    // A different add command cannot use the pending state.
    write_input(tmp.path(), "other.txt", b"Other file.\n");
    let other_out = fieldbook::run(&svec(&["add", "other.txt", "--confirm-public"]), &world);
    assert_eq!(other_out.code, 3);

    // `audit verify` warns FB0803 for the unmatched write.begin.
    let verify_mid = fieldbook::run(&svec(&["audit", "verify", "--json"]), &world);
    assert_eq!(verify_mid.code, 0, "stderr: {}", verify_mid.stderr);
    assert!(
        verify_mid.stdout.contains("FB0803"),
        "audit verify must warn FB0803 while write.begin is unmatched: {}",
        verify_mid.stdout
    );

    // Rerun completes the row and clears the FB0803 warning.
    let second = fieldbook::run(&svec(&["add", "partial.txt", "--confirm-public"]), &world);
    assert_eq!(second.code, 0, "stderr: {}", second.stderr);

    let verify_after = fieldbook::run(&svec(&["audit", "verify", "--json"]), &world);
    assert_eq!(verify_after.code, 0);
    assert!(
        !verify_after.stdout.contains("FB0803"),
        "FB0803 warning must clear after rerun completes: {}",
        verify_after.stdout
    );
}

#[test]
fn no_write_without_journal() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "note.txt", b"Synthetic text.\n");
    let world = TestWorld::new(tmp.path());
    let before = world.read_tree().expect("tree before");

    // Initialize the journal by running an unconfirmed add (creates .fieldbook/audit/journal.jsonl),
    // then make journal.jsonl read-only so Journal::open fails to append before any Corpus write.
    let init = fieldbook::run(&svec(&["add", "note.txt"]), &world);
    assert_eq!(init.code, 1);

    let seg = tmp.path().join(".fieldbook/audit/journal.jsonl");
    let mut perms = std::fs::metadata(&seg).expect("seg metadata").permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&seg, perms).expect("set journal.jsonl readonly");

    let out = fieldbook::run(&svec(&["add", "note.txt", "--confirm-public"]), &world);
    assert_eq!(out.code, 3);
    assert!(
        !tmp.path()
            .join("corpus/sources/public/S-0004-v1.txt")
            .exists()
    );
    assert_eq!(before, world.read_tree().expect("tree after"));
}

#[cfg(windows)]
#[test]
fn held_open_target_is_in_use() {
    use std::os::windows::fs::OpenOptionsExt as _;

    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "note.txt", b"Windows share-mode test.\n");
    let world = TestWorld::new(tmp.path());

    let reg_path = tmp.path().join("corpus/registers/sources.csv");
    let _held = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(&reg_path)
        .expect("open sources.csv with share_mode(0)");

    let out = fieldbook::run(&svec(&["add", "note.txt", "--confirm-public"]), &world);
    assert_eq!(out.code, 3);
    assert!(
        out.stderr
            .contains("is open in another program; close it and run the command again"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn in_use_fault_exits_3_and_names_the_file() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "note.txt", b"In-use fault test.\n");

    let faulty = FaultyWorld::new(
        TestWorld::new(tmp.path()),
        Fault::FailOnName {
            name: "sources.csv",
            kind: FaultKind::InUse,
        },
    );
    let first = fieldbook::run(&svec(&["add", "note.txt", "--confirm-public"]), &faulty);
    assert_eq!(first.code, 3);
    assert!(
        first.stderr.contains("sources.csv")
            && first
                .stderr
                .contains("is open in another program; close it and run the command again"),
        "stderr: {}",
        first.stderr
    );

    let world = TestWorld::new(tmp.path());
    let second = fieldbook::run(&svec(&["add", "note.txt", "--confirm-public"]), &world);
    assert_eq!(second.code, 0, "stderr: {}", second.stderr);
    assert_eq!(second.stdout, "added S-0004-v1 note.txt\n");
}

#[test]
fn a_failed_rename_leaves_no_temporary_file() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "note.txt", b"Failed rename test.\n");
    let world = TestWorld::new(tmp.path());

    // Make the planned path of the original (`corpus/sources/public/S-0004-v1.txt`) an existing
    // non-empty folder so `std::fs::rename` fails inside `io::atomic::write_file`.
    let blocking_dir = tmp.path().join("corpus/sources/public/S-0004-v1.txt");
    std::fs::create_dir_all(&blocking_dir).expect("create blocking dir");
    std::fs::write(blocking_dir.join("keep.txt"), b"keep\n").expect("write keep.txt");

    let out = fieldbook::run(&svec(&["add", "note.txt", "--confirm-public"]), &world);
    assert_eq!(out.code, 3);

    let pub_dir = tmp.path().join("corpus/sources/public");
    for entry in std::fs::read_dir(&pub_dir).expect("read public dir") {
        let entry = entry.expect("dir entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        assert!(
            !name.contains(".tmp-"),
            "leftover temporary file `{name}` found in corpus/sources/public/"
        );
    }
}

#[test]
fn duplicate_without_confirmation_is_refused() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());

    // Copy an already-registered public original (`S-0002-v2.txt`) and run `add` without
    // `--confirm-public`. Labelling and admission must run before `Present` and refuse with FB0410.
    let existing_bytes = std::fs::read(tmp.path().join("corpus/sources/public/S-0002-v2.txt"))
        .expect("read S-0002-v2");
    write_input(tmp.path(), "dup.txt", &existing_bytes);
    let before = world.read_tree().expect("tree before");

    let out = fieldbook::run(&svec(&["add", "dup.txt"]), &world);
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "refused dup.txt\n");
    assert!(out.stderr.contains("FB0410"), "stderr: {}", out.stderr);
    assert_eq!(before, world.read_tree().expect("tree after"));

    let refuse_json = audit_show_json(&world, &["--kind", "refuse"]);
    assert_eq!(refuse_json["events"].as_array().expect("events").len(), 1);
}

#[test]
fn duplicate_marked_text_is_refused() {
    let tmp = temp_copy(&fixture_root());
    let marked_bytes = b"CONTROLLED UNCLASSIFIED INFORMATION\nPre-seeded marked text.\n";
    let marked_sha = Sha256::digest(marked_bytes);

    // Pre-seed the matching hash into sources.csv.
    let reg_path = tmp.path().join("corpus/registers/sources.csv");
    let mut csv_text = std::fs::read_to_string(&reg_path).expect("read sources.csv");
    csv_text.push_str(&format!(
        "S-0004,1,,,,,,,,,,marked.txt,{marked_sha},,,,2026-03-01,active,,,public,,,,,,,\n"
    ));
    std::fs::write(&reg_path, csv_text).expect("write sources.csv");

    write_input(tmp.path(), "marked.txt", marked_bytes);
    let world = TestWorld::new(tmp.path());
    let before = world.read_tree().expect("tree before");

    let out = fieldbook::run(&svec(&["add", "marked.txt", "--confirm-public"]), &world);
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "refused marked.txt\n");
    assert!(out.stderr.contains("FB0400"), "stderr: {}", out.stderr);
    assert_eq!(before, world.read_tree().expect("tree after"));

    let refuse_json = audit_show_json(&world, &["--kind", "refuse"]);
    let events = refuse_json["events"].as_array().expect("events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["code"], "FB0400");
    assert_eq!(events[0]["reason"], "store");
}

#[test]
fn typed_public_act_is_refused_with_a_company_store() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "note.txt", b"Plain synthetic text.\n");
    let world = TestWorld::new(tmp.path());
    world.set_company_store(true);
    let before = world.read_tree().expect("tree before");

    let out = fieldbook::run(&svec(&["add", "note.txt", "--confirm-public"]), &world);
    assert_eq!(out.code, 1);
    assert_eq!(out.stdout, "refused note.txt\n");
    assert!(out.stderr.contains("FB0822"), "stderr: {}", out.stderr);
    assert_eq!(before, world.read_tree().expect("tree after"));

    let refuse_json = audit_show_json(&world, &["--kind", "refuse"]);
    let events = refuse_json["events"].as_array().expect("events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["code"], "FB0822");
    assert_eq!(events[0]["reason"], "label");

    let begin_json = audit_show_json(&world, &["--kind", "write.begin"]);
    assert_eq!(begin_json["events"].as_array().expect("events").len(), 0);
}

#[test]
fn source_id_exhaustion_writes_nothing() {
    let tmp = temp_copy(&fixture_root());

    // First, add S-9999-v1 using a FaultyWorld so we can verify matching recovery at the limit,
    // after seeding S-9998 in sources.csv.
    let reg_path = tmp.path().join("corpus/registers/sources.csv");
    let base_csv = std::fs::read_to_string(&reg_path).expect("read sources.csv");
    let mut seeded_9998 = base_csv;
    let dummy_sha = "b".repeat(64);
    seeded_9998.push_str(&format!(
        "S-9998,1,,,,,,,,,,limit9998.txt,{dummy_sha},,,,2026-03-01,active,,,public,,,,,,,\n"
    ));
    std::fs::write(&reg_path, &seeded_9998).expect("seed S-9998");

    write_input(tmp.path(), "limit9999.txt", b"Source 9999 content.\n");
    let faulty = FaultyWorld::new(
        TestWorld::new(tmp.path()),
        Fault::FailOnName {
            name: "sources.csv",
            kind: FaultKind::Io,
        },
    );
    let interrupted = fieldbook::run(
        &svec(&["add", "limit9999.txt", "--confirm-public"]),
        &faulty,
    );
    assert_eq!(interrupted.code, 3);

    // Matching recovery of S-9999-v1 succeeds at the limit!
    let world = TestWorld::new(tmp.path());
    let recovered = fieldbook::run(&svec(&["add", "limit9999.txt", "--confirm-public"]), &world);
    assert_eq!(recovered.code, 0, "stderr: {}", recovered.stderr);
    assert_eq!(recovered.stdout, "added S-9999-v1 limit9999.txt\n");

    // Duplicate (`Present`) of S-9999-v1 still succeeds when the source space is full through 9999.
    let dup = fieldbook::run(&svec(&["add", "limit9999.txt", "--confirm-public"]), &world);
    assert_eq!(dup.code, 0);
    assert_eq!(dup.stdout, "present S-9999-v1 limit9999.txt\n");

    // Now the public source space is filled through 9999; adding a new source must exit 2 with
    // "source id space exhausted", make no content change, and append no write.begin.
    write_input(tmp.path(), "overflow.txt", b"One source too many.\n");
    let tree_before = world.read_tree().expect("tree before exhaustion");
    let begins_before = audit_show_json(&world, &["--kind", "write.begin"])["events"]
        .as_array()
        .expect("events")
        .len();

    let exhausted = fieldbook::run(&svec(&["add", "overflow.txt", "--confirm-public"]), &world);
    assert_eq!(exhausted.code, 2);
    assert!(
        exhausted.stderr.contains("source id space exhausted"),
        "stderr: {}",
        exhausted.stderr
    );
    assert_eq!(
        tree_before,
        world.read_tree().expect("tree after exhaustion")
    );
    let begins_after = audit_show_json(&world, &["--kind", "write.begin"])["events"]
        .as_array()
        .expect("events")
        .len();
    assert_eq!(begins_before, begins_after);
}

#[test]
fn first_add_initializes_state() {
    let tmp = temp_copy(&fixture_root());
    let state_dir = tmp.path().join(".fieldbook");
    if state_dir.exists() {
        std::fs::remove_dir_all(&state_dir).expect("remove .fieldbook");
    }
    assert!(!state_dir.exists());

    write_input(tmp.path(), "first.txt", b"First add initializes state.\n");
    let world = TestWorld::new(tmp.path());

    let out = fieldbook::run(&svec(&["add", "first.txt", "--confirm-public"]), &world);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(out.stdout, "added S-0004-v1 first.txt\n");

    assert!(state_dir.join("lock").is_file(), "state/lock must exist");
    assert!(
        state_dir.join("store.lock").is_file(),
        "state/store.lock must exist"
    );
    assert!(
        state_dir.join("generation").is_file(),
        "state/generation must exist"
    );
    assert!(
        !state_dir.join("pending.toml").exists(),
        "state/pending.toml must be removed after commit"
    );

    let verify = fieldbook::run(&svec(&["audit", "verify"]), &world);
    assert_eq!(verify.code, 0, "stderr: {}", verify.stderr);

    let begins = audit_show_json(&world, &["--kind", "write.begin"]);
    let ends = audit_show_json(&world, &["--kind", "write.end"]);
    assert_eq!(begins["events"].as_array().expect("begins").len(), 1);
    assert_eq!(ends["events"].as_array().expect("ends").len(), 1);
}

#[test]
fn completed_cells_preserve_reserved_defaults() {
    let tmp = temp_copy(&fixture_root());
    let world = TestWorld::new(tmp.path());
    let tree = world.read_tree().expect("read tree");
    let host = Host::new(tree, world.today());
    let (model, _) = check::load(&host, &check::Options::new(false));
    let existing_rows_before = model.register_rows(Register::Sources).to_vec();

    let mut raw = BTreeMap::new();
    raw.insert("title".to_owned(), "Edited Title, With Comma".to_owned());
    raw.insert("publisher".to_owned(), "Synthetic Publisher".to_owned());
    raw.insert("source_kind".to_owned(), "official_data".to_owned());
    raw.insert("authority_tier".to_owned(), "A".to_owned());
    raw.insert("licence".to_owned(), "public-domain".to_owned());
    raw.insert("published".to_owned(), "2026-01".to_owned());
    raw.insert("effective_from".to_owned(), "2026-02-01".to_owned());
    raw.insert("volatility".to_owned(), "V2".to_owned());
    raw.insert("tag".to_owned(), "syn-tag".to_owned());
    raw.insert("purpose".to_owned(), "test completed cells".to_owned());

    let completed = CompletedSourceCells::parse(&raw).expect("parse valid CompletedSourceCells");
    let sha = "c".repeat(64);
    let AddOutcome::Planned(plan) = plan_add(
        &model,
        "completed.txt",
        &sha,
        &AddOptions::default(),
        Some(&completed),
        world.today(),
    )
    .expect("plan_add with completed cells") else {
        panic!("expected Planned");
    };

    // Planned metadata hash must equal the exact sources_csv payload bytes.
    let reg_wf = plan
        .files
        .iter()
        .find(|f| f.path == "corpus/registers/sources.csv")
        .expect("sources.csv WriteFile");
    assert_eq!(reg_wf.sha256, Sha256::digest(&plan.sources_csv).as_str());

    // All existing rows survive intact, and the appended row preserves reserved defaults.
    let parsed_table = read_csv(&plan.sources_csv).expect("parse planned sources_csv");
    assert_eq!(parsed_table.rows.len(), existing_rows_before.len() + 1);
    for (idx, old_row) in existing_rows_before.iter().enumerate() {
        assert_eq!(parsed_table.rows[idx].cells, old_row.cells);
    }

    let appended = parsed_table.rows.last().expect("appended row");
    assert_eq!(appended.cells[0], "S-0004");
    assert_eq!(appended.cells[1], "1");
    assert_eq!(appended.cells[2], "syn-tag");
    assert_eq!(appended.cells[3], "Edited Title, With Comma");
    assert_eq!(appended.cells[4], "Synthetic Publisher");
    assert_eq!(appended.cells[5], "official_data");
    assert_eq!(appended.cells[6], ""); // reserved source_role
    assert_eq!(appended.cells[7], "A");
    assert_eq!(appended.cells[8], "public-domain");
    assert_eq!(appended.cells[9], ""); // reserved source_family
    assert_eq!(appended.cells[10], ""); // reserved family_basis
    assert_eq!(appended.cells[11], "completed.txt");
    assert_eq!(appended.cells[12], sha);
    assert_eq!(appended.cells[13], "2026-01");
    assert_eq!(appended.cells[14], "2026-02-01");
    assert_eq!(appended.cells[15], ""); // reserved as_of
    assert_eq!(appended.cells[16], world.today().to_string());
    assert_eq!(appended.cells[17], "active");
    assert_eq!(appended.cells[18], ""); // reserved supersedes
    assert_eq!(appended.cells[19], "V2");
    assert_eq!(appended.cells[20], "public");
    assert_eq!(appended.cells[21], ""); // reserved note
    assert_eq!(appended.cells[22], "test completed cells");
    assert_eq!(appended.cells[23], ""); // reserved origin_system
    assert_eq!(appended.cells[24], ""); // reserved origin_stream
    assert_eq!(appended.cells[25], ""); // reserved origin_key
    assert_eq!(appended.cells[26], ""); // reserved doc_kind
    assert_eq!(appended.cells[27], ""); // reserved series

    // Reserved fields and invalid cells must be rejected by CompletedSourceCells::parse.
    for &reserved in &RESERVED_SOURCE_FIELDS {
        let mut map = BTreeMap::new();
        map.insert(reserved.to_owned(), "x".to_owned());
        assert_eq!(
            CompletedSourceCells::parse(&map).unwrap_err(),
            AddError::ReservedField(reserved.to_owned())
        );
    }

    let mut invalid_tier = raw.clone();
    invalid_tier.insert("authority_tier".to_owned(), "B".to_owned()); // official_data allows A or D only
    assert!(matches!(
        CompletedSourceCells::parse(&invalid_tier).unwrap_err(),
        AddError::InvalidCell {
            column: "authority_tier",
            ..
        }
    ));
}

#[test]
fn direct_add_row_bytes_are_unchanged() {
    let tmp = temp_copy(&fixture_root());
    let content = b"Direct add byte-identical row check.\n";
    let sha = Sha256::digest(content);
    write_input(tmp.path(), "direct.txt", content);
    let world = TestWorld::new(tmp.path());

    let before_csv =
        std::fs::read(tmp.path().join("corpus/registers/sources.csv")).expect("read before csv");

    let out = fieldbook::run(&svec(&["add", "direct.txt", "--confirm-public"]), &world);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);

    let after_csv =
        std::fs::read(tmp.path().join("corpus/registers/sources.csv")).expect("read after csv");
    assert!(
        after_csv.starts_with(&before_csv),
        "direct add must preserve existing sources.csv bytes as an exact prefix"
    );

    let base = SourceRow::new(
        SourceId::parse("S-0004").expect("source id"),
        1,
        "direct.txt",
        sha.as_str(),
        world.today(),
    );
    let expected_row = render_source_row(&base, None).expect("render direct row");
    let literal_row = format!(
        "S-0004,1,,,,,,,,,,direct.txt,{sha},,,,{},active,,,public,,,,,,,\n",
        world.today()
    );
    assert_eq!(expected_row, literal_row.as_bytes());
    assert_eq!(&after_csv[before_csv.len()..], expected_row.as_slice());
}
