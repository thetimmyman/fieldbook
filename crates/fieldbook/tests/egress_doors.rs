//! Integration tests for the two writer doors (`io::atomic`), `RootPath` construction,
//! delivery-bound outside writes, `Journal::begin_export`, containment resolution, and
//! `state_scope` lease retention (`core-policy` steps 11, 15, 16, 19, 23, 24).

#![expect(
    clippy::disallowed_methods,
    reason = "integration tests prepare temporary directories, symlinks, and inspect written files"
)]

mod support;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::time::Duration;

use fieldbook::io::atomic::{write_in_root, write_outside};
use fieldbook::io::journal::{
    JournalError, acquire_read_lease, set_lock_timeout_for_test, set_write_pause_hook,
};
use fieldbook::io::state;
use fieldbook::{Error, World};
use fieldbook_core::CivilDate;
use fieldbook_core::audit::{
    DestKind, Event, EventPayload, LabelSet, Now, OperationIntent, OperationKey, RecordRef, Sha256,
    Verb, Via, Word, WriteFile, WriteSet, request_sha256,
};
use fieldbook_core::egress::{Decision, Delivery, Dest, Shipped, Transfer, check};
use fieldbook_core::policy::{EffectivePolicy, FloorKind, OwnerSettings, PolicyState};

use support::temp_copy;
use support::world::TestWorld;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn corpus_mini() -> PathBuf {
    repo_root().join("tests/fixtures/corpus-mini")
}

fn default_now() -> Now {
    let d = CivilDate::from_ymd(2026, 3, 1).unwrap_or_else(|e| panic!("{e}"));
    Now::new(d, 43_200).unwrap_or_else(|e| panic!("{e}"))
}

fn floor_a_effective() -> EffectivePolicy {
    EffectivePolicy::new(&PolicyState::Floor(FloorKind::A), &OwnerSettings::unset())
}

fn make_intent(verb_str: &str, tag: &[u8]) -> OperationIntent {
    OperationIntent {
        schema: 1,
        key: OperationKey::new(
            Verb::parse(verb_str).unwrap_or_else(|e| panic!("{e}")),
            Sha256::digest(tag),
        ),
        request_sha256: request_sha256(&[("req", tag)]).unwrap_or_else(|e| panic!("{e}")),
        allocated: vec![RecordRef::parse("corpus/S-0001").unwrap_or_else(|e| panic!("{e}"))],
        allocated_store_uid: None,
        planned_on: CivilDate::from_ymd(2026, 3, 1).unwrap_or_else(|e| panic!("{e}")),
    }
}

/// Scan `io/atomic.rs` source text for `pub fn` items and return `(name, signature_text)` pairs.
fn scan_atomic_pub_fns(src: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let lines: Vec<&str> = src.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        if !trimmed.starts_with("//")
            && let Some(after_pub_fn) = trimmed.strip_prefix("pub fn ")
        {
            let name_end = after_pub_fn
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .unwrap_or(after_pub_fn.len());
            let name = after_pub_fn[..name_end].to_owned();
            let mut sig = String::new();
            let mut j = i;
            while j < lines.len() {
                sig.push_str(lines[j]);
                sig.push(' ');
                if lines[j].contains('{') {
                    break;
                }
                j += 1;
            }
            out.push((name, sig));
        }
        i += 1;
    }
    out
}

/// Validate that the scanned `pub fn` items of `io/atomic.rs` are exactly `write_in_root` and
/// `write_outside` and that both take `&Audited`. Returns `Err` naming any unexpected or
/// malformed function.
fn check_two_writer_doors(src: &str) -> Result<(), String> {
    let fns = scan_atomic_pub_fns(src);
    for (name, sig) in &fns {
        if name != "write_in_root" && name != "write_outside" {
            return Err(format!(
                "unexpected public writer function `{name}` in io/atomic.rs"
            ));
        }
        if !sig.contains("&Audited") {
            return Err(format!(
                "public writer function `{name}` does not take `&Audited`"
            ));
        }
    }
    let names: Vec<&str> = fns.iter().map(|(n, _)| n.as_str()).collect();
    if names != ["write_in_root", "write_outside"] {
        return Err(format!(
            "expected [\"write_in_root\", \"write_outside\"], found {names:?}"
        ));
    }
    Ok(())
}

/// Scan `(rel_path, source_text)` pairs for `RootPath::new(` outside comments and return the
/// relative paths that contain a match.
fn scan_root_path_makers<'a>(files: &[(&'a str, &'a str)]) -> Vec<&'a str> {
    let mut hits = Vec::new();
    for &(rel_path, content) in files {
        let has_call = content.lines().any(|line| {
            let code = line.split("//").next().unwrap_or("");
            code.contains("RootPath::new(")
        });
        if has_call {
            hits.push(rel_path);
        }
    }
    hits
}

fn collect_rs_files(base: &Path, cur: &Path, out: &mut Vec<(String, String)>) {
    let entries =
        std::fs::read_dir(cur).unwrap_or_else(|e| panic!("read_dir {}: {e}", cur.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|e| panic!("{e}"));
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(base, &path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let rel = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            out.push((rel, text));
        }
    }
}

// control: EG-01
#[test]
fn io_has_two_writer_doors() {
    let atomic_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/io/atomic.rs");
    let src = std::fs::read_to_string(&atomic_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", atomic_path.display()));
    if let Err(msg) = check_two_writer_doors(&src) {
        panic!("{msg}");
    }
}

#[test]
fn scan_finds_a_third_public_writer() {
    let synthetic = r#"
pub fn write_in_root(root: &RootPath, rel: &Path, bytes: &[u8], audited: &Audited) -> Result<Sha256, Error> {
    todo!()
}
pub fn write_outside(allowed: &Allowed, path: &Path, bytes: &[u8], audited: &Audited) -> Result<Sha256, Error> {
    todo!()
}
pub fn write_raw_escape(path: &Path, bytes: &[u8]) -> Result<Sha256, Error> {
    todo!()
}
"#;
    let err = check_two_writer_doors(synthetic).expect_err("third pub fn must fail scan");
    assert!(
        err.contains("write_raw_escape"),
        "error must name the third function, got: {err}"
    );
}

// control: EG-01
#[test]
fn root_path_has_one_maker() {
    let src_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs_files(&src_dir, &src_dir, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(rel, text)| (rel.as_str(), text.as_str()))
        .collect();
    let makers = scan_root_path_makers(&refs);
    assert_eq!(
        makers,
        vec!["io/root.rs"],
        "RootPath::new( must appear only in io/root.rs, found in: {makers:?}"
    );
}

#[test]
fn scan_finds_a_second_maker() {
    let synthetic = [
        ("io/root.rs", "let r = RootPath::new(p, StoreId::Corpus);\n"),
        (
            "add.rs",
            "let forged = RootPath::new(p, StoreId::Corpus);\n",
        ),
    ];
    let makers = scan_root_path_makers(&synthetic);
    assert_eq!(makers, vec!["io/root.rs", "add.rs"]);
}

#[test]
fn state_scope_cannot_write_the_corpus() {
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    let lease = world.write_lease().unwrap_or_else(|e| panic!("{e}"));
    let journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let state_audited = journal
        .state_scope(&lease)
        .unwrap_or_else(|e| panic!("{e}"));

    // 1. State-only Audited with the Corpus RootPath gives Error::OutOfScope from write_in_root.
    let corpus_target = Path::new("corpus/sources/public/forged.txt");
    let res_corpus = write_in_root(
        &world.roots().corpus,
        corpus_target,
        b"forged corpus write\n",
        &state_audited,
    );
    assert!(
        matches!(res_corpus, Err(Error::OutOfScope(_))),
        "expected Error::OutOfScope for Corpus RootPath with state-only Audited, got {res_corpus:?}"
    );
    assert!(!tmp.path().join(corpus_target).exists());

    // 2. State-only Audited also gives Error::OutOfScope from write_outside even with a valid Allowed token.
    let out_tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let out_file = out_tmp.path().join("export.txt");
    let body = b"public export bytes\n";
    let delivery = Delivery {
        entries: vec![Transfer::write_path(out_file.clone(), body)],
        markings: None,
    };
    let Decision::Allowed(allowed) = check(
        &floor_a_effective(),
        &Dest::Export {
            path: out_file.to_string_lossy().into_owned(),
        },
        Some(&LabelSet::public()),
        Some(&delivery),
        default_now(),
        Shipped::Never,
    ) else {
        panic!("expected Allowed");
    };

    let res_outside = write_outside(&allowed, &out_file, body, &state_audited);
    assert!(
        matches!(res_outside, Err(Error::OutOfScope(_))),
        "expected Error::OutOfScope from write_outside with state-only Audited, got {res_outside:?}"
    );
    assert!(!out_file.exists());
}

#[test]
fn selection_token_cannot_write() {
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    let lease = world.write_lease().unwrap_or_else(|e| panic!("{e}"));
    let mut journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let intent = make_intent("add", b"sel-token");
    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new(
            "corpus/sources/public/S-0004-v1.txt",
            Sha256::digest(b"in-root\n").as_str().to_owned(),
        )],
        affected: vec![RecordRef::parse("corpus/S-0001").unwrap_or_else(|e| panic!("{e}"))],
    };
    let audited = journal
        .begin(&lease, &intent, &ws)
        .unwrap_or_else(|e| panic!("{e}"));

    let out_tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let out_file = out_tmp.path().join("selection.txt");

    // Selection token (None, None).
    let Decision::Allowed(sel_token) = check(
        &floor_a_effective(),
        &Dest::Export {
            path: out_file.to_string_lossy().into_owned(),
        },
        None,
        None,
        default_now(),
        Shipped::Never,
    ) else {
        panic!("expected selection Allowed");
    };
    let res_sel = write_outside(&sel_token, &out_file, b"hello\n", &audited);
    assert!(
        matches!(res_sel, Err(Error::OutOfScope(_))),
        "selection token must be refused at write_outside, got {res_sel:?}"
    );
    assert!(!out_file.exists());

    // Preflight token (Some(labels), None).
    let Decision::Allowed(pre_token) = check(
        &floor_a_effective(),
        &Dest::Export {
            path: out_file.to_string_lossy().into_owned(),
        },
        Some(&LabelSet::public()),
        None,
        default_now(),
        Shipped::Never,
    ) else {
        panic!("expected preflight Allowed");
    };
    let res_pre = write_outside(&pre_token, &out_file, b"hello\n", &audited);
    assert!(
        matches!(res_pre, Err(Error::OutOfScope(_))),
        "preflight token must be refused at write_outside, got {res_pre:?}"
    );
    assert!(!out_file.exists());
}

#[test]
fn changed_body_token_reuse_is_refused() {
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    let lease = world.write_lease().unwrap_or_else(|e| panic!("{e}"));
    let mut journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let intent = make_intent("add", b"changed-body");
    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new(
            "corpus/sources/public/S-0004-v1.txt",
            Sha256::digest(b"in-root\n").as_str().to_owned(),
        )],
        affected: vec![RecordRef::parse("corpus/S-0001").unwrap_or_else(|e| panic!("{e}"))],
    };
    let audited = journal
        .begin(&lease, &intent, &ws)
        .unwrap_or_else(|e| panic!("{e}"));

    let out_tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let out_file = out_tmp.path().join("approved.txt");
    let approved_bytes = b"approved exact payload\n";
    let tampered_bytes = b"tampered replacement!!\n";
    assert_eq!(approved_bytes.len(), tampered_bytes.len());

    let delivery = Delivery {
        entries: vec![Transfer::write_path(out_file.clone(), approved_bytes)],
        markings: None,
    };
    let Decision::Allowed(allowed) = check(
        &floor_a_effective(),
        &Dest::Export {
            path: out_file.to_string_lossy().into_owned(),
        },
        Some(&LabelSet::public()),
        Some(&delivery),
        default_now(),
        Shipped::Never,
    ) else {
        panic!("expected Allowed");
    };

    let res = write_outside(&allowed, &out_file, tampered_bytes, &audited);
    assert!(
        matches!(res, Err(Error::OutOfScope(_))),
        "tampered body must be refused with OutOfScope, got {res:?}"
    );
    assert!(!out_file.exists());
}

#[test]
fn cross_path_token_reuse_is_refused() {
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    let lease = world.write_lease().unwrap_or_else(|e| panic!("{e}"));
    let mut journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let intent = make_intent("add", b"cross-path");
    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new(
            "corpus/sources/public/S-0004-v1.txt",
            Sha256::digest(b"in-root\n").as_str().to_owned(),
        )],
        affected: vec![RecordRef::parse("corpus/S-0001").unwrap_or_else(|e| panic!("{e}"))],
    };
    let audited = journal
        .begin(&lease, &intent, &ws)
        .unwrap_or_else(|e| panic!("{e}"));

    let out_tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let path_a = out_tmp.path().join("a.txt");
    let path_b = out_tmp.path().join("b.txt");
    let body = b"exact payload\n";

    let delivery = Delivery {
        entries: vec![Transfer::write_path(path_a.clone(), body)],
        markings: None,
    };
    let Decision::Allowed(allowed) = check(
        &floor_a_effective(),
        &Dest::Export {
            path: path_a.to_string_lossy().into_owned(),
        },
        Some(&LabelSet::public()),
        Some(&delivery),
        default_now(),
        Shipped::Never,
    ) else {
        panic!("expected Allowed");
    };

    let res = write_outside(&allowed, &path_b, body, &audited);
    assert!(
        matches!(res, Err(Error::OutOfScope(_))),
        "reusing token for a different path must fail with OutOfScope, got {res:?}"
    );
    assert!(!path_b.exists());
}

#[test]
fn approved_delivery_writes_exact_bytes() {
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    let out_tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let out_file = out_tmp.path().join("pack.md");
    let body = b"# Public Pack\n\nSynthetic public content.\n";
    let body_sha = Sha256::digest(body);

    let delivery = Delivery {
        entries: vec![Transfer::write_path(out_file.clone(), body)],
        markings: None,
    };
    let Decision::Allowed(allowed) = check(
        &floor_a_effective(),
        &Dest::Export {
            path: out_file.to_string_lossy().into_owned(),
        },
        Some(&LabelSet::public()),
        Some(&delivery),
        default_now(),
        Shipped::Never,
    ) else {
        panic!("expected Allowed");
    };

    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new("pack.md", body_sha.as_str().to_owned())],
        affected: vec![RecordRef::parse("corpus/S-0001").unwrap_or_else(|e| panic!("{e}"))],
    };
    let mut journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let audited = journal
        .begin_export("context", &ws, &allowed, &out_file)
        .unwrap_or_else(|e| panic!("{e}"));

    let written_sha = world
        .write_outside(&allowed, &audited, &out_file, body)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(written_sha, body_sha);
    assert_eq!(
        std::fs::read(&out_file).unwrap_or_else(|e| panic!("{e}")),
        body
    );

    journal
        .end(audited, Word::new_static("ok"))
        .unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn export_begin_and_end_keep_bound_context() {
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    let out_tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let out_file = out_tmp.path().join("pack.md");
    let body = b"Exported public context.\n";
    let body_sha = Sha256::digest(body);

    let delivery = Delivery {
        entries: vec![Transfer::write_path(out_file.clone(), body)],
        markings: None,
    };
    let eff = floor_a_effective();
    let Decision::Allowed(allowed) = check(
        &eff,
        &Dest::Export {
            path: out_file.to_string_lossy().into_owned(),
        },
        Some(&LabelSet::public()),
        Some(&delivery),
        default_now(),
        Shipped::Never,
    ) else {
        panic!("expected Allowed");
    };

    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new("pack.md", body_sha.as_str().to_owned())],
        affected: vec![RecordRef::parse("corpus/S-0001").unwrap_or_else(|e| panic!("{e}"))],
    };

    let mut journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let audited = journal
        .begin_export("context", &ws, &allowed, &out_file)
        .unwrap_or_else(|e| panic!("{e}"));
    world
        .write_outside(&allowed, &audited, &out_file, body)
        .unwrap_or_else(|e| panic!("{e}"));
    journal
        .end(audited, Word::new_static("ok"))
        .unwrap_or_else(|e| panic!("{e}"));

    let raw = std::fs::read_to_string(world.state_dir().join("audit/journal.jsonl"))
        .unwrap_or_else(|e| panic!("{e}"));
    let events: Vec<Event> = raw
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| Event::parse_line(l).unwrap_or_else(|e| panic!("{e}")).0)
        .collect();

    let expected_policy = Sha256::parse(allowed.policy_hash()).unwrap_or_else(|e| panic!("{e}"));
    let begin_ev = events
        .iter()
        .find(|e| matches!(e.payload, EventPayload::WriteBegin { .. }))
        .expect("write.begin event");
    let end_ev = events
        .iter()
        .find(|e| matches!(e.payload, EventPayload::WriteEnd { .. }))
        .expect("write.end event");

    let EventPayload::WriteBegin {
        verb: b_verb,
        labels: b_labels,
        file_hashes,
        dest: b_dest,
        path_sha256: b_path_sha,
        policy: b_policy,
        ..
    } = &begin_ev.payload
    else {
        panic!("expected WriteBegin");
    };
    assert_eq!(b_verb.as_str(), "context");
    assert_eq!(b_labels, &LabelSet::public());
    assert!(
        file_hashes.is_none(),
        "export write.begin must not have file_hashes"
    );
    assert_eq!(*b_dest, Some(DestKind::Export));
    assert!(b_path_sha.is_some());
    assert_eq!(b_policy.as_ref(), Some(&expected_policy));

    let EventPayload::WriteEnd {
        verb: e_verb,
        labels: e_labels,
        outcome,
        dest: e_dest,
        path_sha256: e_path_sha,
        policy: e_policy,
        ..
    } = &end_ev.payload
    else {
        panic!("expected WriteEnd");
    };
    assert_eq!(e_verb.as_str(), "context");
    assert_eq!(e_labels, &LabelSet::public());
    assert_eq!(outcome.as_str(), "ok");
    assert_eq!(*e_dest, Some(DestKind::Export));
    assert_eq!(e_path_sha, b_path_sha);
    assert_eq!(e_policy.as_ref(), Some(&expected_policy));
}

#[test]
fn selection_or_mismatched_set_cannot_begin_export() {
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    let out_tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let out_file = out_tmp.path().join("pack.md");
    let body = b"Exported public context.\n";
    let body_sha = Sha256::digest(body);

    let mut journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new("pack.md", body_sha.as_str().to_owned())],
        affected: vec![RecordRef::parse("corpus/S-0001").unwrap_or_else(|e| panic!("{e}"))],
    };

    // 1. Preflight / selection token (no delivery) is refused by begin_export.
    let Decision::Allowed(preflight) = check(
        &floor_a_effective(),
        &Dest::Export {
            path: out_file.to_string_lossy().into_owned(),
        },
        Some(&LabelSet::public()),
        None,
        default_now(),
        Shipped::Never,
    ) else {
        panic!("expected preflight Allowed");
    };
    assert!(
        journal
            .begin_export("context", &ws, &preflight, &out_file)
            .is_err(),
        "preflight token without delivery must be refused by begin_export"
    );

    // 2. Delivery-bound token with mismatched WriteSet hash or labels is refused by begin_export.
    let delivery = Delivery {
        entries: vec![Transfer::write_path(out_file.clone(), body)],
        markings: None,
    };
    let Decision::Allowed(allowed) = check(
        &floor_a_effective(),
        &Dest::Export {
            path: out_file.to_string_lossy().into_owned(),
        },
        Some(&LabelSet::public()),
        Some(&delivery),
        default_now(),
        Shipped::Never,
    ) else {
        panic!("expected Allowed");
    };

    let bad_hash_ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new(
            "pack.md",
            Sha256::digest(b"different").as_str().to_owned(),
        )],
        affected: ws.affected.clone(),
    };
    assert!(
        journal
            .begin_export("context", &bad_hash_ws, &allowed, &out_file)
            .is_err(),
        "mismatched file hash in WriteSet must be refused by begin_export"
    );

    let bad_labels_ws = WriteSet {
        labels: LabelSet::owner_supplied(),
        files: ws.files.clone(),
        affected: ws.affected.clone(),
    };
    assert!(
        journal
            .begin_export("context", &bad_labels_ws, &allowed, &out_file)
            .is_err(),
        "mismatched labels in WriteSet must be refused by begin_export"
    );

    // Neither refused case appended a write.begin event.
    let raw = std::fs::read_to_string(world.state_dir().join("audit/journal.jsonl"))
        .unwrap_or_else(|e| panic!("{e}"));
    for line in raw.lines().filter(|l| !l.is_empty()) {
        let (ev, _) = Event::parse_line(line).unwrap_or_else(|e| panic!("{e}"));
        assert!(
            !matches!(ev.payload, EventPayload::WriteBegin { .. }),
            "refused begin_export must not append write.begin, found: {ev:?}"
        );
    }
}

#[test]
fn failed_export_does_not_block_store_reads() {
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    let out_tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let out_file = out_tmp.path().join("pack.md");
    let body = b"Exported public context.\n";
    let body_sha = Sha256::digest(body);

    // Create a directory at `out_file` so the outward write fails after `begin_export`.
    std::fs::create_dir_all(out_file.join("blocker")).unwrap_or_else(|e| panic!("{e}"));

    let delivery = Delivery {
        entries: vec![Transfer::write_path(out_file.clone(), body)],
        markings: None,
    };
    let Decision::Allowed(allowed) = check(
        &floor_a_effective(),
        &Dest::Export {
            path: out_file.to_string_lossy().into_owned(),
        },
        Some(&LabelSet::public()),
        Some(&delivery),
        default_now(),
        Shipped::Never,
    ) else {
        panic!("expected Allowed");
    };

    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new("pack.md", body_sha.as_str().to_owned())],
        affected: vec![RecordRef::parse("corpus/S-0001").unwrap_or_else(|e| panic!("{e}"))],
    };

    let mut journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let audited = journal
        .begin_export("context", &ws, &allowed, &out_file)
        .unwrap_or_else(|e| panic!("{e}"));

    // `begin_export` must never create `state/pending.toml`.
    assert!(!world.state_dir().join("pending.toml").exists());

    // Fail the outward write and drop `audited` without calling `journal.end` (simulating an
    // interrupted export).
    let write_res = world.write_outside(&allowed, &audited, &out_file, body);
    assert!(
        write_res.is_err(),
        "outward write to blocking dir must fail"
    );
    drop(audited);

    // Input store must remain readable (no pending.toml blocking `read_tree`).
    let tree = world
        .read_tree()
        .expect("input store must remain readable after failed export");
    assert!(tree.get_bytes("corpus/corpus.toml").is_some());

    // And the unmatched write.begin event remains visible in the journal.
    let raw = std::fs::read_to_string(world.state_dir().join("audit/journal.jsonl"))
        .unwrap_or_else(|e| panic!("{e}"));
    let has_begin = raw.lines().filter(|l| !l.is_empty()).any(|l| {
        matches!(
            Event::parse_line(l).unwrap().0.payload,
            EventPayload::WriteBegin { .. }
        )
    });
    assert!(has_begin, "unmatched write.begin event must remain visible");
}

#[test]
fn in_root_writes_resolve_containment_before_mutation() {
    let tmp = temp_copy(&corpus_mini());
    let outside_tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let existing_outside = outside_tmp.path().join("existing.txt");
    std::fs::write(&existing_outside, b"untouched\n").unwrap_or_else(|e| panic!("{e}"));

    let world = TestWorld::new(tmp.path());
    let lease = world.write_lease().unwrap_or_else(|e| panic!("{e}"));
    let mut journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let intent = make_intent("add", b"containment");
    let ws = WriteSet {
        labels: LabelSet::public(),
        files: vec![WriteFile::new(
            "corpus/sources/public/S-0004-v1.txt",
            Sha256::digest(b"permitted\n").as_str().to_owned(),
        )],
        affected: vec![RecordRef::parse("corpus/S-0001").unwrap_or_else(|e| panic!("{e}"))],
    };
    let audited = journal
        .begin(&lease, &intent, &ws)
        .unwrap_or_else(|e| panic!("{e}"));

    // 1. Permitted missing destination inside root succeeds.
    let permitted_rel = Path::new("corpus/sources/public/new-nested/S-0004-v1.txt");
    let sha = write_in_root(
        &world.roots().corpus,
        permitted_rel,
        b"permitted\n",
        &audited,
    )
    .expect("permitted missing destination must succeed");
    assert_eq!(sha, Sha256::digest(b"permitted\n"));
    assert_eq!(
        std::fs::read(tmp.path().join(permitted_rel)).unwrap(),
        b"permitted\n"
    );

    // 2. `..` escape refuses with OutOfScope before creating any directory or file.
    let dotdot_rel = Path::new("corpus/../../escaped-dotdot/file.txt");
    let res_dotdot = write_in_root(&world.roots().corpus, dotdot_rel, b"bad\n", &audited);
    assert!(
        matches!(res_dotdot, Err(Error::OutOfScope(_))),
        "expected OutOfScope for `..` escape, got {res_dotdot:?}"
    );

    // 3. Symlink-parent escape (and Windows junction-parent escape) refuses with OutOfScope,
    //    leaves existing outside files unchanged, and creates no outside directory or temp file.
    #[cfg(unix)]
    {
        let sym_parent = tmp.path().join("corpus/sym_escape");
        std::os::unix::fs::symlink(outside_tmp.path(), &sym_parent)
            .unwrap_or_else(|e| panic!("symlink: {e}"));

        let res_sym_existing = write_in_root(
            &world.roots().corpus,
            Path::new("corpus/sym_escape/existing.txt"),
            b"overwritten\n",
            &audited,
        );
        assert!(
            matches!(res_sym_existing, Err(Error::OutOfScope(_))),
            "expected OutOfScope for symlink-parent existing target, got {res_sym_existing:?}"
        );
        assert_eq!(
            std::fs::read(&existing_outside).unwrap(),
            b"untouched\n",
            "existing outside file must remain untouched"
        );

        let res_sym_nested = write_in_root(
            &world.roots().corpus,
            Path::new("corpus/sym_escape/new_dir/escaped.txt"),
            b"escaped\n",
            &audited,
        );
        assert!(
            matches!(res_sym_nested, Err(Error::OutOfScope(_))),
            "expected OutOfScope for symlink-parent nested target, got {res_sym_nested:?}"
        );
        assert!(
            !outside_tmp.path().join("new_dir").exists(),
            "no outside directory may be created before containment check"
        );
    }

    #[cfg(windows)]
    {
        let junc_parent = tmp.path().join(r"corpus\junc_escape");
        let status = std::process::Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                &junc_parent.to_string_lossy(),
                &outside_tmp.path().to_string_lossy(),
            ])
            .status()
            .unwrap_or_else(|e| panic!("mklink /J: {e}"));
        assert!(status.success(), "mklink /J must succeed on Windows");

        let res_junc_existing = write_in_root(
            &world.roots().corpus,
            Path::new(r"corpus\junc_escape\existing.txt"),
            b"overwritten\n",
            &audited,
        );
        assert!(
            matches!(res_junc_existing, Err(Error::OutOfScope(_))),
            "expected OutOfScope for Windows junction-parent existing target, got {res_junc_existing:?}"
        );
        assert_eq!(
            std::fs::read(&existing_outside).unwrap(),
            b"untouched\n",
            "existing outside file must remain untouched"
        );

        let res_junc_nested = write_in_root(
            &world.roots().corpus,
            Path::new(r"corpus\junc_escape\new_dir\escaped.txt"),
            b"escaped\n",
            &audited,
        );
        assert!(
            matches!(res_junc_nested, Err(Error::OutOfScope(_))),
            "expected OutOfScope for Windows junction-parent nested target, got {res_junc_nested:?}"
        );
        assert!(
            !outside_tmp.path().join("new_dir").exists(),
            "no outside directory may be created before containment check"
        );
    }

    // Verify no temporary files were created in outside_tmp.
    let outside_entries: Vec<String> = std::fs::read_dir(outside_tmp.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(outside_entries, vec!["existing.txt".to_owned()]);
}

#[test]
fn state_scope_writer_retains_lease() {
    let tmp = temp_copy(&corpus_mini());
    let state_dir = tmp.path().join("state");
    let world = TestWorld::with_state(tmp.path(), &state_dir);

    let lease = world.write_lease().unwrap_or_else(|e| panic!("{e}"));
    let journal = state::open(&world, &state_dir, Via::Cli).unwrap_or_else(|e| panic!("{e}"));

    let raw_before = std::fs::read_to_string(state_dir.join("audit/journal.jsonl"))
        .unwrap_or_else(|e| panic!("{e}"));

    let audited = journal
        .state_scope(&lease)
        .unwrap_or_else(|e| panic!("{e}"));

    // Acquisition of state_scope adds no journal event.
    let raw_after_acq = std::fs::read_to_string(state_dir.join("audit/journal.jsonl"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        raw_before, raw_after_acq,
        "state_scope acquisition must add no event"
    );

    let entered_write = Arc::new(Barrier::new(2));
    let release_write = Arc::new(Barrier::new(2));
    let first_write_done = Arc::new(AtomicBool::new(false));

    let entered_clone = Arc::clone(&entered_write);
    let release_clone = Arc::clone(&release_write);
    set_write_pause_hook(Some(Arc::new(move |_path| {
        entered_clone.wait();
        release_clone.wait();
    })));

    let root_path = tmp.path().to_path_buf();
    let state_path = state_dir.clone();
    let done_flag = Arc::clone(&first_write_done);

    let competing = std::thread::spawn(move || {
        entered_write.wait();
        // While the first state write is paused inside `write_in_root`, release it after a short
        // delay and simultaneously attempt a competing journal lock acquisition via `state::open`.
        let release_thread = {
            let release_write = Arc::clone(&release_write);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(120));
                release_write.wait();
            })
        };
        let w2 = TestWorld::with_state(&root_path, &state_path);
        let _j2 =
            state::open(&w2, &state_path, Via::Cli).expect("competing open after lease release");
        let observed_done = done_flag.load(Ordering::SeqCst);
        release_thread.join().expect("release thread");
        observed_done
    });

    let derived_path = state_dir.join("derived-index.bin");
    let sha = world
        .write_file(&audited, &derived_path, b"state-payload\n")
        .unwrap_or_else(|e| panic!("{e}"));
    set_write_pause_hook(None);
    assert_eq!(sha, Sha256::digest(b"state-payload\n"));
    first_write_done.store(true, Ordering::SeqCst);
    drop(audited);

    let competing_saw_first_done = competing.join().expect("competing thread");
    assert!(
        competing_saw_first_done,
        "competing writer must wait until the StateScope write finishes and releases its lease"
    );
}

#[test]
fn failed_state_scope_acquisition_cannot_write() {
    let tmp = temp_copy(&corpus_mini());
    let state_dir = tmp.path().join("state");
    let world = TestWorld::with_state(tmp.path(), &state_dir);

    let lease = world.write_lease().unwrap_or_else(|e| panic!("{e}"));
    let journal = state::open(&world, &state_dir, Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    drop(lease);

    // 1. Read-only (shared) lease is refused by state_scope.
    let shared_lease = acquire_read_lease(tmp.path(), &state_dir).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        matches!(
            journal.state_scope(&shared_lease),
            Err(JournalError::InvalidLease(_))
        ),
        "shared lease must be refused by state_scope"
    );
    drop(shared_lease);

    // 2. Foreign lease (for another directory) is refused by state_scope.
    let other_tmp = temp_copy(&corpus_mini());
    let other_state = other_tmp.path().join("state");
    let other_world = TestWorld::with_state(other_tmp.path(), &other_state);
    let foreign_lease = other_world.write_lease().unwrap_or_else(|e| panic!("{e}"));
    assert!(
        matches!(
            journal.state_scope(&foreign_lease),
            Err(JournalError::InvalidLease(_))
        ),
        "foreign lease must be refused by state_scope"
    );
    drop(foreign_lease);

    // 3. Timeout while another thread holds the journal lock gives JournalError::Busy and no token.
    let ex_lease = world.write_lease().unwrap_or_else(|e| panic!("{e}"));
    let held_scope = journal
        .state_scope(&ex_lease)
        .unwrap_or_else(|e| panic!("{e}"));
    let journal2 = state::open(&world, &state_dir, Via::Cli);
    // Note: `state::open` itself acquires the journal lock; with a short timeout on another thread
    // while `held_scope` is alive, `state_scope` on a second handle times out with `Busy`.
    drop(journal2);
    drop(held_scope);

    let journal_b = state::open(&world, &state_dir, Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let held_scope = journal
        .state_scope(&ex_lease)
        .unwrap_or_else(|e| panic!("{e}"));
    set_lock_timeout_for_test(Some(Duration::from_millis(150)));
    let busy_res = journal_b.state_scope(&ex_lease);
    set_lock_timeout_for_test(None);
    assert_eq!(busy_res.unwrap_err(), JournalError::Busy);
    drop(held_scope);

    // 4. Invalid journal head is refused by state_scope and leaves state unchanged.
    let seg_path = state_dir.join("audit/journal.jsonl");
    let valid_journal = std::fs::read_to_string(&seg_path).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(&seg_path, format!("{valid_journal}corrupted-tail-line\n"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        matches!(
            journal.state_scope(&ex_lease),
            Err(JournalError::BrokenChain(_))
        ),
        "corrupted journal head must be refused by state_scope"
    );
    std::fs::write(&seg_path, &valid_journal).unwrap_or_else(|e| panic!("{e}"));
    assert!(!state_dir.join("should-not-exist.bin").exists());
}
