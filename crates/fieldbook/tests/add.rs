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

    let forbidden_file_writes = ["fs::write(", "File::create(", "OpenOptions::new()"];
    let forbidden_folder_ops = [
        "fs::remove_dir_all(",
        "fs::create_dir(",
        "fs::create_dir_all(",
        "fs::rename(",
    ];

    for path in entries {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let text = std::fs::read_to_string(&path).expect("read io source file");
        if !matches!(name, "atomic.rs" | "journal.rs") {
            for pat in forbidden_file_writes {
                assert!(
                    !text.contains(pat),
                    "raw file write call `{pat}` found in {name} outside atomic.rs and journal.rs"
                );
            }
        }
        if !matches!(name, "atomic.rs" | "journal.rs" | "dirs.rs") {
            for pat in forbidden_folder_ops {
                assert!(
                    !text.contains(pat),
                    "raw folder operation `{pat}` found in {name} outside atomic.rs, journal.rs and dirs.rs"
                );
            }
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
    use std::sync::Mutex;

    struct HoldOnWrite {
        inner: TestWorld,
        held: Mutex<Option<std::fs::File>>,
    }

    impl World for HoldOnWrite {
        fn read_tree(&self) -> Result<fieldbook_core::check::Tree, fieldbook::WorldError> {
            self.inner.read_tree()
        }
        fn write_lease(&self) -> Result<fieldbook::StoreLease, fieldbook::WorldError> {
            self.inner.write_lease()
        }
        fn read_tree_under(
            &self,
            lease: &fieldbook::StoreLease,
        ) -> Result<fieldbook_core::check::Tree, fieldbook::WorldError> {
            self.inner.read_tree_under(lease)
        }
        fn write_file(
            &self,
            audited: &fieldbook::Audited,
            path: &Path,
            bytes: &[u8],
        ) -> Result<Sha256, fieldbook::WorldError> {
            if path.file_name().and_then(|n| n.to_str()) == Some("sources.csv") {
                let file = std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .share_mode(0)
                    .open(path)
                    .unwrap_or_else(|e| panic!("open sources.csv with share_mode(0): {e}"));
                *self.held.lock().unwrap_or_else(|e| panic!("{e}")) = Some(file);
            }
            self.inner.write_file(audited, path, bytes)
        }
        fn write_outside(
            &self,
            allowed: &fieldbook_core::egress::Allowed,
            audited: &fieldbook::Audited,
            path: &Path,
            bytes: &[u8],
        ) -> Result<Sha256, fieldbook::WorldError> {
            self.inner.write_outside(allowed, audited, path, bytes)
        }
        fn has_company_store(&self) -> bool {
            self.inner.has_company_store()
        }
        fn stdin_is_terminal(&self) -> bool {
            self.inner.stdin_is_terminal()
        }
        fn read_line(&self) -> String {
            self.inner.read_line()
        }
        fn prompt(&self, flag: &str) -> Result<(), fieldbook::WorldError> {
            self.inner.prompt(flag)
        }
        fn today(&self) -> fieldbook_core::CivilDate {
            self.inner.today()
        }
        fn now(&self) -> fieldbook_core::audit::Now {
            self.inner.now()
        }
        fn principal(&self) -> Principal {
            self.inner.principal()
        }
        fn cwd(&self) -> &Path {
            self.inner.cwd()
        }
        fn tree_root(&self) -> &Path {
            self.inner.tree_root()
        }
        fn state_dir(&self) -> &Path {
            self.inner.state_dir()
        }
        fn place(&self, path: &Path) -> fieldbook_core::labels::PlaceFacts {
            self.inner.place(path)
        }
        fn support_facts(&self) -> fieldbook::SupportFacts {
            self.inner.support_facts()
        }
        fn reset_drill(&self, audited: &fieldbook::Audited) -> Result<(), fieldbook::WorldError> {
            self.inner.reset_drill(audited)
        }
        fn remove_drill(&self, audited: &fieldbook::Audited) -> Result<(), fieldbook::WorldError> {
            self.inner.remove_drill(audited)
        }
    }

    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "note.txt", b"Windows share-mode test.\n");
    let world = HoldOnWrite {
        inner: TestWorld::new(tmp.path()),
        held: Mutex::new(None),
    };

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

#[test]
fn folder_report_is_in_byte_order() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "inbox/z-last.txt", b"Zebra synthetic note.\n");
    write_input(tmp.path(), "inbox/A-upper.txt", b"Upper synthetic note.\n");
    write_input(tmp.path(), "inbox/a-lower.txt", b"Lower synthetic note.\n");
    let world = TestWorld::new(tmp.path());

    let out = fieldbook::run(&svec(&["add", "inbox", "--confirm-public"]), &world);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "added     S-0004-v1  A-upper.txt\n",
            "added     S-0005-v1  a-lower.txt\n",
            "added     S-0006-v1  z-last.txt\n",
            "done: 3 added\n",
        )
    );
}

#[test]
fn folder_adds_present_and_refuses() {
    let tmp = temp_copy(&fixture_root());
    let shared = b"Synthetic shared note for folder test.\n";
    write_input(tmp.path(), "inbox/a-new.txt", shared);
    write_input(tmp.path(), "inbox/b-copy.txt", shared);
    write_input(
        tmp.path(),
        "inbox/c-marked.txt",
        b"CONTROLLED UNCLASSIFIED INFORMATION\nSynthetic marked note.\n",
    );
    write_input(
        tmp.path(),
        "inbox/d-script.ps1",
        b"Write-Output 'synthetic'\n",
    );
    let world = TestWorld::new(tmp.path());

    let out = fieldbook::run(&svec(&["add", "inbox", "--confirm-public"]), &world);
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stdout,
        concat!(
            "added     S-0004-v1  a-new.txt\n",
            "present   S-0004-v1  b-copy.txt\n",
            "refused   FB0400  c-marked.txt\n",
            "refused   FB0821  d-script.ps1\n",
            "done: 1 added, 1 present, 2 refused\n",
        )
    );
    assert!(
        tmp.path()
            .join("corpus/sources/public/S-0004-v1.txt")
            .is_file()
    );
    assert!(
        !tmp.path()
            .join("corpus/sources/public/S-0005-v1.txt")
            .exists()
    );
}

#[test]
fn refusal_does_not_stop_the_folder() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "inbox/a-bad.ps1", b"echo synthetic\n");
    write_input(tmp.path(), "inbox/b-good.txt", b"Good synthetic text.\n");
    let world = TestWorld::new(tmp.path());

    let out = fieldbook::run(&svec(&["add", "inbox", "--confirm-public"]), &world);
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stdout,
        concat!(
            "refused   FB0821  a-bad.ps1\n",
            "added     S-0004-v1  b-good.txt\n",
            "done: 1 added, 1 refused\n",
        )
    );
    assert!(
        tmp.path()
            .join("corpus/sources/public/S-0004-v1.txt")
            .is_file()
    );
}

#[test]
fn exit_is_1_when_any_file_is_refused() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "inbox/a-good.txt", b"Good synthetic text.\n");
    write_input(tmp.path(), "inbox/b-bad.sh", b"#!/bin/sh\necho synthetic\n");
    let world = TestWorld::new(tmp.path());

    let out = fieldbook::run(&svec(&["add", "inbox", "--confirm-public"]), &world);
    assert_eq!(out.code, 1);
    assert!(out.stdout.contains("done: 1 added, 1 refused\n"));
}

#[test]
fn io_error_exits_3_and_keeps_earlier_files() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "inbox/a-first.txt", b"First synthetic file.\n");
    write_input(
        tmp.path(),
        "inbox/b-second.txt",
        b"Second synthetic file.\n",
    );
    write_input(tmp.path(), "inbox/c-third.txt", b"Third synthetic file.\n");

    let faulty = FaultyWorld::new(
        TestWorld::new(tmp.path()),
        Fault::FailOnName {
            name: "S-0005-v1.txt",
            kind: FaultKind::Io,
        },
    );

    let out = fieldbook::run(
        &svec(&["add", "inbox", "--confirm-public", "--json"]),
        &faulty,
    );
    assert_eq!(out.code, 3, "stderr: {}", out.stderr);

    // First file was committed; second file failed during its original write; third file was never visited.
    assert!(
        tmp.path()
            .join("corpus/sources/public/S-0004-v1.txt")
            .is_file()
    );
    assert!(
        !tmp.path()
            .join("corpus/sources/public/S-0006-v1.txt")
            .exists()
    );

    let stdout_report: Value = serde_json::from_str(&out.stdout)
        .unwrap_or_else(|e| panic!("parse partial stdout json: {e}; stdout={}", out.stdout));
    assert_eq!(stdout_report["exit"], 3);
    assert_eq!(stdout_report["counts"]["added"], 1);
    assert_eq!(stdout_report["counts"]["present"], 0);
    assert_eq!(stdout_report["counts"]["refused"], 0);
    let files = stdout_report["files"].as_array().expect("files array");
    assert_eq!(
        files.len(),
        1,
        "later files must remain unvisited on I/O error"
    );
    assert_eq!(files[0]["name"], "a-first.txt");
    assert_eq!(files[0]["result"], "added");
    assert_eq!(files[0]["id"], "S-0004-v1");

    // Best-effort saved report in reports/last-add.json must match the partial stdout JSON.
    let saved_raw = std::fs::read_to_string(tmp.path().join(".fieldbook/reports/last-add.json"))
        .expect("read saved partial last-add.json");
    let saved_report: Value =
        serde_json::from_str(&saved_raw).expect("parse saved partial last-add.json");
    assert_eq!(saved_report, stdout_report);
}

#[test]
fn recursive_walks_subfolders() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "inbox/top.txt", b"Top synthetic file.\n");
    write_input(
        tmp.path(),
        "inbox/sub/deep.txt",
        b"Deep synthetic subfolder file.\n",
    );
    let world = TestWorld::new(tmp.path());

    let out = fieldbook::run(
        &svec(&["add", "inbox", "--recursive", "--confirm-public"]),
        &world,
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "added     S-0004-v1  sub/deep.txt\n",
            "added     S-0005-v1  top.txt\n",
            "done: 2 added\n",
        )
    );
}

#[test]
fn without_recursive_ignores_subfolders() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "inbox/top.txt", b"Top synthetic file.\n");
    write_input(
        tmp.path(),
        "inbox/sub/deep.txt",
        b"Deep synthetic subfolder file.\n",
    );
    let world = TestWorld::new(tmp.path());

    let out = fieldbook::run(&svec(&["add", "inbox", "--confirm-public"]), &world);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!("added     S-0004-v1  top.txt\n", "done: 1 added\n")
    );
    assert!(
        !tmp.path()
            .join("corpus/sources/public/S-0005-v1.txt")
            .exists()
    );
}

#[test]
fn json_report_has_version_1() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "inbox/a.txt", b"Synthetic JSON report file.\n");
    let world = TestWorld::new(tmp.path());

    let out = fieldbook::run(
        &svec(&["add", "inbox", "--confirm-public", "--json"]),
        &world,
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let v: Value = serde_json::from_str(&out.stdout).expect("parse add --json");
    assert_eq!(v["report_version"], 1);
    assert_eq!(v["verb"], "add");
    assert_eq!(v["dir"], "inbox");
    assert_eq!(v["exit"], 0);
    assert_eq!(v["counts"]["added"], 1);
}

#[test]
fn last_report_is_written_and_replaced() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "inbox1/one.txt", b"First folder run.\n");
    write_input(tmp.path(), "inbox2/two.txt", b"Second folder run.\n");
    let world = TestWorld::new(tmp.path());
    let report_path = tmp.path().join(".fieldbook/reports/last-add.json");

    let first = fieldbook::run(&svec(&["add", "inbox1", "--confirm-public"]), &world);
    assert_eq!(first.code, 0, "stderr: {}", first.stderr);
    let first_json: Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("read first report"))
            .expect("parse first report");
    assert_eq!(first_json["dir"], "inbox1");
    assert_eq!(first_json["files"][0]["name"], "one.txt");

    let second = fieldbook::run(&svec(&["add", "inbox2", "--confirm-public"]), &world);
    assert_eq!(second.code, 0, "stderr: {}", second.stderr);
    let second_json: Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("read second report"))
            .expect("parse second report");
    assert_eq!(second_json["dir"], "inbox2");
    assert_eq!(second_json["files"][0]["name"], "two.txt");
}

#[test]
fn report_write_failure_is_a_warning() {
    let tmp = temp_copy(&fixture_root());
    write_input(
        tmp.path(),
        "inbox/note.txt",
        b"Synthetic note for report failure.\n",
    );

    let faulty = FaultyWorld::new(
        TestWorld::new(tmp.path()),
        Fault::FailOnName {
            name: "last-add.json",
            kind: FaultKind::Io,
        },
    );
    let out = fieldbook::run(&svec(&["add", "inbox", "--confirm-public"]), &faulty);
    assert_eq!(out.code, 0, "report save failure must not change exit code");
    assert!(
        out.stderr.contains("report not saved"),
        "stderr must warn when report is not saved: {}",
        out.stderr
    );
    assert!(
        tmp.path()
            .join("corpus/sources/public/S-0004-v1.txt")
            .is_file()
    );
}

#[test]
fn needs_extraction_is_true_for_a_pdf() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "inbox/a.txt", b"Plain synthetic text.\n");
    write_input(
        tmp.path(),
        "inbox/b.pdf",
        b"%PDF-1.4\n% Synthetic PDF.\n%%EOF\n",
    );
    let world = TestWorld::new(tmp.path());

    let out = fieldbook::run(
        &svec(&["add", "inbox", "--confirm-public", "--json"]),
        &world,
    );
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    let v: Value = serde_json::from_str(&out.stdout).expect("parse add --json");
    assert_eq!(v["files"][0]["name"], "a.txt");
    assert_eq!(v["files"][0]["needs_extraction"], false);
    assert_eq!(v["files"][1]["name"], "b.pdf");
    assert_eq!(v["files"][1]["needs_extraction"], true);
    assert_eq!(v["counts"]["needs_extraction"], 1);
}

#[test]
fn source_with_a_folder_is_a_usage_error() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "inbox/a.txt", b"Plain synthetic text.\n");
    let world = TestWorld::new(tmp.path());
    let before = world.read_tree().expect("tree before");

    let out = fieldbook::run(
        &svec(&["add", "inbox", "--source", "S-0001", "--confirm-public"]),
        &world,
    );
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("--source"), "stderr: {}", out.stderr);
    assert_eq!(before, world.read_tree().expect("tree after"));
}

#[test]
fn renamed_executable_is_refused() {
    let tmp = temp_copy(&fixture_root());
    write_input(
        tmp.path(),
        "inbox/pe_as_text.txt",
        b"MZ\x90\x00not really text",
    );
    write_input(
        tmp.path(),
        "inbox/elf_as_text.txt",
        b"\x7fELF\x02\x01\x01\x00not text",
    );
    let world = TestWorld::new(tmp.path());
    let before = world.read_tree().expect("tree before");

    let out = fieldbook::run(&svec(&["add", "inbox", "--confirm-public"]), &world);
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stdout,
        concat!(
            "refused   FB0821  elf_as_text.txt\n",
            "refused   FB0821  pe_as_text.txt\n",
            "done: 2 refused\n",
        )
    );
    assert_eq!(before, world.read_tree().expect("tree after"));
}

#[test]
fn each_added_file_has_one_begin_and_end() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "inbox/a.txt", b"First synthetic file.\n");
    write_input(tmp.path(), "inbox/b.txt", b"Second synthetic file.\n");
    let world = TestWorld::new(tmp.path());

    let out = fieldbook::run(&svec(&["add", "inbox", "--confirm-public"]), &world);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);

    let shown = audit_show_json(&world, &[]);
    let events = shown["events"].as_array().expect("events");
    let kinds: Vec<&str> = events.iter().filter_map(|e| e["kind"].as_str()).collect();
    assert_eq!(
        kinds,
        vec![
            "journal.open",
            "settings.change",
            "write.begin",
            "write.end",
            "write.begin",
            "write.end"
        ]
    );
}

#[test]
fn folder_without_confirmation_refuses_every_file_fb0410() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "inbox/a.txt", b"First unconfirmed file.\n");
    write_input(tmp.path(), "inbox/b.txt", b"Second unconfirmed file.\n");
    let world = TestWorld::new(tmp.path());
    let before = world.read_tree().expect("tree before");

    let out = fieldbook::run(&svec(&["add", "inbox"]), &world);
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stdout,
        concat!(
            "refused   FB0410  a.txt\n",
            "refused   FB0410  b.txt\n",
            "done: 2 refused\n",
        )
    );
    assert_eq!(before, world.read_tree().expect("tree after"));

    let begins = audit_show_json(&world, &["--kind", "write.begin"]);
    assert_eq!(begins["events"].as_array().expect("events").len(), 0);
}

#[test]
fn one_confirmation_covers_the_whole_folder() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "inbox/a.txt", b"First confirmed file.\n");
    write_input(tmp.path(), "inbox/b.txt", b"Second confirmed file.\n");
    write_input(tmp.path(), "inbox/c.md", b"# Third confirmed file\n");
    let world = TestWorld::new(tmp.path());

    let out = fieldbook::run(&svec(&["add", "inbox", "--confirm-public"]), &world);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        concat!(
            "added     S-0004-v1  a.txt\n",
            "added     S-0005-v1  b.txt\n",
            "added     S-0006-v1  c.md\n",
            "done: 3 added\n",
        )
    );
}

#[test]
fn macro_types_are_refused_in_the_corpus_and_pass_for_another_store() {
    use fieldbook_core::add::check_type;
    use fieldbook_core::audit::StoreId;
    use fieldbook_core::check::Code;

    for name in ["sheet.xlsm", "LETTER.DOCM", "deck.pptm"] {
        let err = check_type(name, b"PK\x03\x04", StoreId::Corpus)
            .expect_err("macro Office file must be refused in Corpus");
        assert_eq!(err.code, Code::Fb0821);
        assert_eq!(err.file_name, name);

        check_type(name, b"PK\x03\x04", StoreId::Overlay)
            .expect("macro Office file must be allowed in Overlay store");
    }

    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "inbox/vendor.xlsm", b"PK\x03\x04synthetic zip");
    let world = TestWorld::new(tmp.path());
    let out = fieldbook::run(&svec(&["add", "inbox", "--confirm-public"]), &world);
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stdout,
        concat!("refused   FB0821  vendor.xlsm\n", "done: 1 refused\n")
    );
}

#[test]
fn each_refusal_has_one_refuse_event() {
    // 1. Folder with FB0821 (script), FB0400 (marking), and FB0410 (unconfirmed plain file).
    let tmp = temp_copy(&fixture_root());
    let ps1_bytes = b"Write-Output 'synthetic'\n";
    let marked_bytes = b"CONTROLLED UNCLASSIFIED INFORMATION\nSynthetic marked text.\n";
    let plain_bytes = b"Plain unconfirmed text.\n";
    write_input(tmp.path(), "inbox/a-tool.ps1", ps1_bytes);
    write_input(tmp.path(), "inbox/b-marked.txt", marked_bytes);
    write_input(tmp.path(), "inbox/c-plain.txt", plain_bytes);
    let world = TestWorld::new(tmp.path());

    let out = fieldbook::run(&svec(&["add", "inbox"]), &world);
    assert_eq!(out.code, 1);

    let refuse_json = audit_show_json(&world, &["--kind", "refuse"]);
    let events = refuse_json["events"].as_array().expect("events");
    assert_eq!(
        events.len(),
        3,
        "expected one refuse event per refused file"
    );

    // Event 0: FB0821 (type refusal before classification -> no fabricated labels).
    assert_eq!(events[0]["code"], "FB0821");
    assert_eq!(events[0]["reason"], "store");
    assert_eq!(events[0]["sha256"], Sha256::digest(ps1_bytes).as_str());
    assert_eq!(
        events[0]["name_sha256"],
        Sha256::digest(b"a-tool.ps1").as_str()
    );
    assert!(
        events[0].get("labels").is_none(),
        "FB0821 occurs before classification and must not fabricate labels: {}",
        events[0]
    );

    // Event 1: FB0400 (marking refusal -> reason `store`, labels `["marked"]`).
    assert_eq!(events[1]["code"], "FB0400");
    assert_eq!(events[1]["reason"], "store");
    assert_eq!(events[1]["sha256"], Sha256::digest(marked_bytes).as_str());
    assert_eq!(
        events[1]["name_sha256"],
        Sha256::digest(b"b-marked.txt").as_str()
    );
    assert_eq!(events[1]["labels"], serde_json::json!(["marked"]));

    // Event 2: FB0410 (unconfirmed plain file -> reason `unlabelled`, labels `["unlabelled"]`).
    assert_eq!(events[2]["code"], "FB0410");
    assert_eq!(events[2]["reason"], "unlabelled");
    assert_eq!(events[2]["sha256"], Sha256::digest(plain_bytes).as_str());
    assert_eq!(
        events[2]["name_sha256"],
        Sha256::digest(b"c-plain.txt").as_str()
    );
    assert_eq!(events[2]["labels"], serde_json::json!(["unlabelled"]));

    let begins = audit_show_json(&world, &["--kind", "write.begin"]);
    assert_eq!(begins["events"].as_array().expect("events").len(), 0);

    let verify = fieldbook::run(&svec(&["audit", "verify"]), &world);
    assert_eq!(verify.code, 0, "stderr: {}", verify.stderr);

    // 2. FB0822 (--confirm-public on a machine with a company store -> reason `label`, labels `["public"]`).
    let tmp_co = temp_copy(&fixture_root());
    write_input(tmp_co.path(), "inbox/c-plain.txt", plain_bytes);
    let world_co = TestWorld::new(tmp_co.path());
    world_co.set_company_store(true);

    let out_co = fieldbook::run(&svec(&["add", "inbox", "--confirm-public"]), &world_co);
    assert_eq!(out_co.code, 1);

    let refuse_co = audit_show_json(&world_co, &["--kind", "refuse"]);
    let events_co = refuse_co["events"].as_array().expect("events");
    assert_eq!(events_co.len(), 1);
    assert_eq!(events_co[0]["code"], "FB0822");
    assert_eq!(events_co[0]["reason"], "label");
    assert_eq!(events_co[0]["sha256"], Sha256::digest(plain_bytes).as_str());
    assert_eq!(
        events_co[0]["name_sha256"],
        Sha256::digest(b"c-plain.txt").as_str()
    );
    assert_eq!(events_co[0]["labels"], serde_json::json!(["public"]));

    let begins_co = audit_show_json(&world_co, &["--kind", "write.begin"]);
    assert_eq!(begins_co["events"].as_array().expect("events").len(), 0);

    let verify_co = fieldbook::run(&svec(&["audit", "verify"]), &world_co);
    assert_eq!(verify_co.code, 0, "stderr: {}", verify_co.stderr);
}

#[test]
fn state_add_folder_matches_the_writer() {
    let dir = tempfile::tempdir().expect("tempdir");
    let events = &support::state::state_mini_events()[..2];
    support::state::build_state(dir.path(), events);

    let expected_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/state-add-folder/audit/journal.jsonl");
    let expected = std::fs::read_to_string(&expected_path).expect("read state-add-folder journal");
    let actual = std::fs::read_to_string(dir.path().join("audit/journal.jsonl"))
        .expect("read built journal");
    assert_eq!(actual, expected);
}

#[test]
fn folder_rerun_recovers_a_later_file() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "inbox/a-first.txt", b"First synthetic file.\n");
    write_input(
        tmp.path(),
        "inbox/b-second.txt",
        b"Second synthetic file.\n",
    );

    // Fail on the 2nd sources.csv write (after a-first.txt succeeded and b-second.txt's original
    // S-0005-v1.txt was written).
    let faulty = FaultyWorld::new(
        TestWorld::new(tmp.path()),
        Fault::FailOnNthName {
            name: "sources.csv",
            nth: 2,
            kind: FaultKind::Io,
        },
    );
    let interrupted = fieldbook::run(&svec(&["add", "inbox", "--confirm-public"]), &faulty);
    assert_eq!(interrupted.code, 3);
    assert!(tmp.path().join(".fieldbook/pending.toml").is_file());
    assert!(
        tmp.path()
            .join("corpus/sources/public/S-0004-v1.txt")
            .is_file()
    );
    assert!(
        tmp.path()
            .join("corpus/sources/public/S-0005-v1.txt")
            .is_file()
    );

    // Rerun the exact folder command on a healthy World:
    // - pre-traversal recovers b-second.txt (Added S-0005-v1),
    // - ordinary traversal reports a-first.txt as Present S-0004-v1 and skips b-second.txt,
    // - final output is in byte order of filename.
    let world = TestWorld::new(tmp.path());
    let recovered = fieldbook::run(&svec(&["add", "inbox", "--confirm-public"]), &world);
    assert_eq!(recovered.code, 0, "stderr: {}", recovered.stderr);
    assert_eq!(
        recovered.stdout,
        concat!(
            "present   S-0004-v1  a-first.txt\n",
            "added     S-0005-v1  b-second.txt\n",
            "done: 1 added, 1 present\n",
        )
    );
    assert!(!tmp.path().join(".fieldbook/pending.toml").exists());
    assert!(
        !tmp.path()
            .join("corpus/sources/public/S-0006-v1.txt")
            .exists()
    );

    let sources_bytes =
        std::fs::read(tmp.path().join("corpus/registers/sources.csv")).expect("read sources.csv");
    let table = read_csv(&sources_bytes).expect("parse sources.csv");
    assert_eq!(
        table.rows.len(),
        6,
        "must have 3 initial rows + 2 added rows"
    );

    let begins = audit_show_json(&world, &["--kind", "write.begin"]);
    let ends = audit_show_json(&world, &["--kind", "write.end"]);
    assert_eq!(
        begins["events"].as_array().expect("begins").len(),
        2,
        "recovery must not append an extra write.begin"
    );
    assert_eq!(ends["events"].as_array().expect("ends").len(), 2);
}

#[test]
fn folder_recovery_refuses_missing_or_changed_candidate() {
    let tmp = temp_copy(&fixture_root());
    write_input(tmp.path(), "inbox/a-first.txt", b"First synthetic file.\n");
    let second_path = write_input(
        tmp.path(),
        "inbox/b-second.txt",
        b"Second synthetic file.\n",
    );

    let faulty = FaultyWorld::new(
        TestWorld::new(tmp.path()),
        Fault::FailOnNthName {
            name: "sources.csv",
            nth: 2,
            kind: FaultKind::Io,
        },
    );
    let interrupted = fieldbook::run(&svec(&["add", "inbox", "--confirm-public"]), &faulty);
    assert_eq!(interrupted.code, 3);

    let world = TestWorld::new(tmp.path());
    let begins_before = audit_show_json(&world, &["--kind", "write.begin"])["events"]
        .as_array()
        .expect("events")
        .len();

    // Case 1: Mutate the second candidate while pending -> must refuse with exit 3 before any write.
    std::fs::write(&second_path, b"Tampered second candidate.\n").expect("tamper second file");
    let changed_out = fieldbook::run(&svec(&["add", "inbox", "--confirm-public"]), &world);
    assert_eq!(changed_out.code, 3);
    assert!(tmp.path().join(".fieldbook/pending.toml").is_file());

    // Case 2: Remove the second candidate while pending -> must refuse with exit 3 before any write.
    std::fs::remove_file(&second_path).expect("remove second file");
    let missing_out = fieldbook::run(&svec(&["add", "inbox", "--confirm-public"]), &world);
    assert_eq!(missing_out.code, 3);
    assert!(tmp.path().join(".fieldbook/pending.toml").is_file());

    let begins_after = audit_show_json(&world, &["--kind", "write.begin"])["events"]
        .as_array()
        .expect("events")
        .len();
    assert_eq!(begins_before, begins_after);

    // Restore the second candidate and confirm recovery succeeds cleanly.
    std::fs::write(&second_path, b"Second synthetic file.\n").expect("restore second file");
    let recovered = fieldbook::run(&svec(&["add", "inbox", "--confirm-public"]), &world);
    assert_eq!(recovered.code, 0, "stderr: {}", recovered.stderr);
    assert!(!tmp.path().join(".fieldbook/pending.toml").exists());
}

#[test]
fn report_state_admission_failure_preserves_previous() {
    let tmp = temp_copy(&fixture_root());
    write_input(
        tmp.path(),
        "inbox1/clean.txt",
        b"Clean public synthetic text.\n",
    );
    let world = TestWorld::new(tmp.path());
    let report_path = tmp.path().join(".fieldbook/reports/last-add.json");

    // 1. First run saves a valid last-add.json.
    let first = fieldbook::run(&svec(&["add", "inbox1", "--confirm-public"]), &world);
    assert_eq!(first.code, 0, "stderr: {}", first.stderr);
    let saved_before = std::fs::read_to_string(&report_path).expect("read initial last-add.json");

    // 2. Second run inspects a file with a marking banner (`marked`), which FloorKind::A denies in
    // the State store (`StoreRule::floor(StoreId::State, false)`). The add command exits 1 for the
    // refused file, warns `report not saved` on stderr, and leaves the previous last-add.json intact.
    write_input(
        tmp.path(),
        "inbox2/marked.txt",
        b"CONTROLLED UNCLASSIFIED INFORMATION\nSynthetic marked text.\n",
    );
    let second = fieldbook::run(&svec(&["add", "inbox2", "--confirm-public"]), &world);
    assert_eq!(second.code, 1);
    assert!(
        second.stderr.contains("report not saved"),
        "stderr must warn when state admission denies report: {}",
        second.stderr
    );
    let saved_after = std::fs::read_to_string(&report_path).expect("read preserved last-add.json");
    assert_eq!(
        saved_after, saved_before,
        "previous report must be preserved"
    );
}

#[test]
fn saved_report_requires_classification() {
    use fieldbook::report::parse_saved_add_report;

    let valid = r#"{
  "report_version": 1,
  "verb": "add",
  "dir": "inbox",
  "labels": ["owner-supplied"],
  "inputs": [{ "store": "corpus", "id": "S-0004-v1" }],
  "files": [
    {
      "name": "a.txt",
      "result": "added",
      "id": "S-0004-v1",
      "sha256": "1aa0592a6ef1b53921b5131ff20cb88ee9762c37b66ecf13f02b69c584a2cf76",
      "bytes": 35,
      "needs_extraction": false
    }
  ],
  "counts": { "added": 1, "present": 0, "refused": 0, "needs_extraction": 0 },
  "exit": 0
}"#;
    let parsed = parse_saved_add_report(valid).expect("valid saved report must parse");
    assert_eq!(parsed.files[0].name, "a.txt");

    // Missing `labels`
    let no_labels = valid.replace(r#"  "labels": ["owner-supplied"],"#, "");
    assert!(parse_saved_add_report(&no_labels).is_err());

    // Empty `labels`
    let empty_labels = valid.replace(r#"["owner-supplied"]"#, "[]");
    assert!(parse_saved_add_report(&empty_labels).is_err());

    // Missing `inputs`
    let no_inputs = valid.replace(
        r#"  "inputs": [{ "store": "corpus", "id": "S-0004-v1" }],"#,
        "",
    );
    assert!(parse_saved_add_report(&no_inputs).is_err());

    // Added file missing from `inputs`
    let missing_input_ref = valid.replace(
        r#"  "inputs": [{ "store": "corpus", "id": "S-0004-v1" }],"#,
        r#"  "inputs": [],"#,
    );
    assert!(parse_saved_add_report(&missing_input_ref).is_err());
}

#[test]
fn folder_recovery_distinguishes_equal_basenames() {
    let tmp = temp_copy(&fixture_root());
    let shared = b"Identical content under two subfolders.\n";
    write_input(tmp.path(), "inbox/a/note.txt", shared);
    write_input(tmp.path(), "inbox/b/note.txt", shared);

    // Interrupt the first register write (`inbox/a/note.txt`).
    let faulty = FaultyWorld::new(
        TestWorld::new(tmp.path()),
        Fault::FailOnNthName {
            name: "sources.csv",
            nth: 1,
            kind: FaultKind::Io,
        },
    );
    let interrupted = fieldbook::run(
        &svec(&["add", "inbox", "--recursive", "--confirm-public"]),
        &faulty,
    );
    assert_eq!(interrupted.code, 3);
    assert!(tmp.path().join(".fieldbook/pending.toml").is_file());

    // Rerun: canonical input-path hash uniquely matches `inbox/a/note.txt` (not `inbox/b/note.txt`).
    // `a/note.txt` recovers as Added S-0004-v1, and `b/note.txt` sees S-0004-v1 as Present.
    let world = TestWorld::new(tmp.path());
    let recovered = fieldbook::run(
        &svec(&["add", "inbox", "--recursive", "--confirm-public"]),
        &world,
    );
    assert_eq!(recovered.code, 0, "stderr: {}", recovered.stderr);
    assert_eq!(
        recovered.stdout,
        concat!(
            "added     S-0004-v1  a/note.txt\n",
            "present   S-0004-v1  b/note.txt\n",
            "done: 1 added, 1 present\n",
        )
    );
    assert!(!tmp.path().join(".fieldbook/pending.toml").exists());
    assert!(
        tmp.path()
            .join("corpus/sources/public/S-0004-v1.txt")
            .is_file()
    );
    assert!(
        !tmp.path()
            .join("corpus/sources/public/S-0005-v1.txt")
            .exists()
    );

    let sources_bytes =
        std::fs::read(tmp.path().join("corpus/registers/sources.csv")).expect("read sources.csv");
    let table = read_csv(&sources_bytes).expect("parse sources.csv");
    assert_eq!(
        table.rows.len(),
        5,
        "must have 4 initial rows + 1 added row"
    );

    let begins = audit_show_json(&world, &["--kind", "write.begin"]);
    let ends = audit_show_json(&world, &["--kind", "write.end"]);
    assert_eq!(begins["events"].as_array().expect("begins").len(), 1);
    assert_eq!(ends["events"].as_array().expect("ends").len(), 1);
}

#[test]
fn add_dot_imports_current_directory() {
    let tmp = temp_copy(&fixture_root());
    std::fs::create_dir_all(tmp.path().join("inbox/sub")).expect("create inbox/sub");
    write_input(
        tmp.path(),
        "inbox/dot-note.txt",
        b"Public note inside current directory.\n",
    );
    let world = TestWorld::new(tmp.path());
    // Both `.` and `inbox/sub/..` have `Path::file_name() == None`.
    let out = fieldbook::run(&svec(&["add", "inbox/sub/..", "--confirm-public"]), &world);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "added     S-0004-v1  dot-note.txt\ndone: 1 added\n"
    );
}

#[test]
fn recursive_folder_refuses_marked_subdirectory_name() {
    let tmp = temp_copy(&fixture_root());
    write_input(
        tmp.path(),
        "inbox/PROPRIETARY/note.txt",
        b"Unmarked body inside a marked directory.\n",
    );
    let world = TestWorld::new(tmp.path());
    let out = fieldbook::run(
        &svec(&["add", "inbox", "--recursive", "--confirm-public"]),
        &world,
    );
    assert_eq!(out.code, 1, "marked subdirectory must be refused");
    assert!(
        out.stderr.contains("FB0400"),
        "stderr must report FB0400: {}",
        out.stderr
    );
    let sources_csv = std::fs::read_to_string(tmp.path().join("corpus/registers/sources.csv"))
        .expect("read sources.csv");
    assert!(
        !sources_csv.contains("PROPRIETARY"),
        "sources.csv must not persist marked directory path"
    );
}
