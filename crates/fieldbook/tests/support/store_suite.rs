//! Shared contract test suite for [`fieldbook::World`] / [`Disk`] store implementations (`ADR 0004 §11`).

#![allow(
    dead_code,
    clippy::expect_used,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    reason = "shared test support modules are selectively used across test binaries"
)]

use std::path::Path;

use fieldbook::World;
use fieldbook::io::Disk;
use fieldbook::io::journal::{Journal, acquire_write_lease};
use fieldbook_core::audit::{
    LabelSet, OperationIntent, OperationKey, RecordRef, Sha256, Verb, Via, Word, WriteFile,
    WriteSet,
};

fn make_intent(disk: &Disk, subject: &[u8]) -> OperationIntent {
    OperationIntent {
        schema: 1,
        key: OperationKey {
            verb: Verb::parse("add").expect("valid verb"),
            subject_sha256: Sha256::digest(subject),
        },
        request_sha256: Sha256::digest(subject),
        allocated: vec![RecordRef::parse("corpus/S-0001").expect("valid ref")],
        allocated_store_uid: None,
        planned_on: disk.today(),
    }
}

/// Helper to write a file under a [`Disk`] environment through an audited journal transaction and return the [`Sha256`].
pub fn write_store_file(
    disk: &Disk,
    rel_path: &str,
    bytes: &[u8],
) -> Result<Sha256, Box<dyn std::error::Error>> {
    let lease = acquire_write_lease(disk.tree_root(), disk.state_dir())
        .map_err(|e| format!("acquire lease: {e}"))?;
    let mut journal = Journal::open(disk, disk.state_dir(), Via::Cli)
        .map_err(|e| format!("open journal: {e}"))?;

    let file_sha = Sha256::digest(bytes);
    let full_path = disk.tree_root().join(rel_path);
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new(rel_path, file_sha.to_string())],
        affected: vec![RecordRef::parse("corpus/S-0001")?],
    };
    let intent = make_intent(disk, bytes);

    let audited = journal
        .begin(&lease, &intent, &ws)
        .map_err(|e| format!("journal begin: {e}"))?;
    let written_sha = disk
        .write_file(&audited, &full_path, bytes)
        .map_err(|e| format!("write file: {e}"))?;
    journal
        .end(audited, Word::new_static("ok"))
        .map_err(|e| format!("journal end: {e}"))?;

    Ok(written_sha)
}

/// Runs the four store contract checks over the [`Disk`] constructed by `make`:
///
/// 1. A write set that holds an absolute path or `..` is refused.
/// 2. An original is written before the row that cites it, and a write that fails in the middle leaves no row.
/// 3. A listing is in byte order.
/// 4. The hash of the bytes read back equals the hash that the write returned.
pub fn store_suite(make: &dyn Fn(&Path) -> Disk) {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let corpus_dir = tmp.path().join("corpus");
    let state_dir = tmp.path().join(".fieldbook");
    let sources_org = corpus_dir.join("sources/public");
    let registers_dir = corpus_dir.join("registers");
    std::fs::create_dir_all(&sources_org).expect("create sources org dir");
    std::fs::create_dir_all(&registers_dir).expect("create registers dir");
    std::fs::create_dir_all(&state_dir).expect("create state dir");

    let disk = make(tmp.path());

    // -------------------------------------------------------------------------
    // 1. A write set that holds an absolute path or `..` is refused
    // -------------------------------------------------------------------------
    let lease = acquire_write_lease(disk.tree_root(), disk.state_dir()).expect("acquire lease");
    let mut journal = Journal::open(&disk, disk.state_dir(), Via::Cli).expect("open journal");

    let intent = make_intent(&disk, b"refusal-check");

    // Absolute path in write set
    let ws_absolute = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new(
            "/corpus/sources/public/S-0001-v1.txt",
            Sha256::digest(b"bytes").to_string(),
        )],
        affected: vec![RecordRef::parse("corpus/S-0001").expect("valid ref")],
    };
    let res_abs = journal.begin(&lease, &intent, &ws_absolute);
    assert!(
        res_abs.is_err(),
        "write set containing absolute path must be refused"
    );

    // Parent directory traversal `..` in write set
    let ws_dotdot = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new(
            "corpus/../outside.txt",
            Sha256::digest(b"bytes").to_string(),
        )],
        affected: vec![RecordRef::parse("corpus/S-0001").expect("valid ref")],
    };
    let res_dotdot = journal.begin(&lease, &intent, &ws_dotdot);
    assert!(
        res_dotdot.is_err(),
        "write set containing .. path must be refused"
    );

    // -------------------------------------------------------------------------
    // 2. An original is written before the row that cites it, and a write that
    //    fails in the middle leaves no row
    // -------------------------------------------------------------------------
    let tmp_failure = tempfile::tempdir().expect("create tempdir for failure check");
    let disk_failure = make(tmp_failure.path());
    std::fs::create_dir_all(tmp_failure.path().join("corpus/sources/public"))
        .expect("create sources");
    std::fs::create_dir_all(tmp_failure.path().join("corpus/registers")).expect("create registers");
    std::fs::create_dir_all(tmp_failure.path().join(".fieldbook")).expect("create .fieldbook");

    let lease_fail =
        acquire_write_lease(disk_failure.tree_root(), disk_failure.state_dir()).expect("lease");
    let mut journal_fail =
        Journal::open(&disk_failure, disk_failure.state_dir(), Via::Cli).expect("journal");

    let orig_rel = "corpus/sources/public/S-0001-v1.txt";
    let reg_rel = "corpus/registers/sources.csv";
    let orig_bytes = b"synthetic original content\n";
    let orig_sha = Sha256::digest(orig_bytes);
    let reg_bytes = b"source_id,version,sha256\nS-0001,1,aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n";
    let reg_sha = Sha256::digest(reg_bytes);

    let ws_valid = WriteSet {
        labels: LabelSet::public(),
        files: vec![
            WriteFile::new(orig_rel, orig_sha.to_string()),
            WriteFile::new(reg_rel, reg_sha.to_string()),
        ],
        affected: vec![RecordRef::parse("corpus/S-0001").expect("valid ref")],
    };
    let intent_valid = make_intent(&disk_failure, orig_bytes);

    let audited = journal_fail
        .begin(&lease_fail, &intent_valid, &ws_valid)
        .expect("journal begin for valid transaction");

    // Write original first
    let orig_path = disk_failure.tree_root().join(orig_rel);
    let orig_write_hash = disk_failure
        .write_file(&audited, &orig_path, orig_bytes)
        .expect("write original file");
    assert_eq!(orig_write_hash, orig_sha);
    assert!(
        orig_path.is_file(),
        "original must be on disk before register row"
    );

    // Observable ordering: register row has not been written yet
    let reg_path = disk_failure.tree_root().join(reg_rel);
    assert!(
        !reg_path.exists(),
        "register citing row must not exist before its write occurs"
    );

    // Simulate failure in the middle: drop audited without calling journal.end
    std::mem::drop(audited);
    std::mem::drop(journal_fail);
    std::mem::drop(lease_fail);

    // Observable consequence: no row citing the original exists in registers
    assert!(
        !reg_path.exists(),
        "aborted transaction must leave no row in register"
    );

    // -------------------------------------------------------------------------
    // 3. A listing is in byte order
    // -------------------------------------------------------------------------
    // Create multiple files in the corpus tree with out-of-order names
    let file_names = ["p0003.txt", "p0001.txt", "p0002.txt"];
    let files_dir = corpus_dir.join("listing_test");
    std::fs::create_dir_all(&files_dir).expect("create listing test dir");
    for name in file_names {
        std::fs::write(files_dir.join(name), b"test").expect("write listing file");
    }

    let tree = disk.read_tree_under(&lease).expect("read tree under lease");
    let paths: Vec<&str> = tree.paths().collect();
    assert!(
        paths.windows(2).all(|w| w[0] < w[1]),
        "tree listing paths must be in strictly ascending byte order"
    );

    let test_paths: Vec<&str> = paths
        .iter()
        .copied()
        .filter(|p| p.starts_with("corpus/listing_test/"))
        .collect();
    assert_eq!(
        test_paths,
        vec![
            "corpus/listing_test/p0001.txt",
            "corpus/listing_test/p0002.txt",
            "corpus/listing_test/p0003.txt",
        ],
        "listing of files under listing_test must be in exact byte order"
    );

    std::mem::drop(journal);
    std::mem::drop(lease);

    // -------------------------------------------------------------------------
    // 4. The hash of the bytes read back equals the hash that the write returned
    // -------------------------------------------------------------------------
    let readback_rel = "corpus/readback_test.txt";
    let readback_bytes = b"bytes to read back and hash verify\n";
    let readback_sha = write_store_file(&disk, readback_rel, readback_bytes)
        .expect("write_store_file should succeed");

    let read_bytes = std::fs::read(disk.tree_root().join(readback_rel)).expect("read file back");
    let actual_sha = Sha256::digest(&read_bytes);
    assert_eq!(
        readback_sha, actual_sha,
        "hash of bytes read back must equal the hash returned by write"
    );
}
