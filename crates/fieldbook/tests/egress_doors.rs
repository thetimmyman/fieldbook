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
    AtomicStage, JournalError, acquire_read_lease, set_atomic_stage_hook,
    set_lock_timeout_for_test, set_write_pause_hook,
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

#[cfg(unix)]
#[test]
fn write_outside_rejects_post_binding_symlink_swap() {
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());

    let dest_tmp = tempfile::tempdir().unwrap();
    let folder = dest_tmp.path().join("folder");
    std::fs::create_dir_all(&folder).unwrap();

    let elsewhere_tmp = tempfile::tempdir().unwrap();
    let existing_outside = elsewhere_tmp.path().join("existing.txt");
    std::fs::write(&existing_outside, b"untouched\n").unwrap();

    let target_existing = folder.join("existing.txt");
    let target_nested = folder.join("new_dir/escaped.txt");
    let payload = b"overwritten\n";
    let payload_sha = Sha256::digest(payload);

    let delivery = Delivery {
        entries: vec![
            Transfer::write_path(target_existing.clone(), payload),
            Transfer::write_path(target_nested.clone(), payload),
        ],
        markings: None,
    };
    let Decision::Allowed(allowed) = check(
        &floor_a_effective(),
        &Dest::Export {
            path: dest_tmp.path().to_string_lossy().into_owned(),
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
        files: vec![
            WriteFile::new("folder/existing.txt", payload_sha.as_str().to_owned()),
            WriteFile::new(
                "folder/new_dir/escaped.txt",
                payload_sha.as_str().to_owned(),
            ),
        ],
        affected: vec![RecordRef::parse("corpus/S-0001").unwrap_or_else(|e| panic!("{e}"))],
    };
    let mut journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let audited = journal
        .begin_export("context", &ws, &allowed, dest_tmp.path())
        .unwrap_or_else(|e| panic!("{e}"));

    // 1. Directory component swapped for a symlink after `egress::check` issued `Allowed`.
    std::fs::remove_dir(&folder).unwrap();
    std::os::unix::fs::symlink(elsewhere_tmp.path(), &folder).unwrap();

    let res_swap_existing = write_outside(&allowed, &target_existing, payload, &audited);
    assert!(
        matches!(res_swap_existing, Err(Error::OutOfScope(_))),
        "expected OutOfScope when parent directory was swapped for a symlink after binding, got {res_swap_existing:?}"
    );
    assert_eq!(
        std::fs::read(&existing_outside).unwrap(),
        b"untouched\n",
        "existing outside file must remain untouched after post-binding symlink swap"
    );

    let res_swap_nested = write_outside(&allowed, &target_nested, payload, &audited);
    assert!(
        matches!(res_swap_nested, Err(Error::OutOfScope(_))),
        "expected OutOfScope for nested target under swapped symlink parent, got {res_swap_nested:?}"
    );
    assert!(
        !elsewhere_tmp.path().join("new_dir").exists(),
        "no outside directory may be created under swapped symlink parent"
    );

    // Passing the symlink target path directly must also fail (bound_path must not re-resolve through the symlink).
    let res_direct_elsewhere = write_outside(&allowed, &existing_outside, payload, &audited);
    assert!(
        matches!(res_direct_elsewhere, Err(Error::OutOfScope(_))),
        "expected OutOfScope when addressing symlink target directly, got {res_direct_elsewhere:?}"
    );
    assert_eq!(
        std::fs::read(&existing_outside).unwrap(),
        b"untouched\n",
        "existing outside file must remain untouched"
    );

    // 2. Final file component replaced with a symlink (live or dangling) inside a real folder.
    std::fs::remove_file(&folder).unwrap();
    std::fs::create_dir_all(&folder).unwrap();
    std::os::unix::fs::symlink(&existing_outside, &target_existing).unwrap();

    let res_sym_file = write_outside(&allowed, &target_existing, payload, &audited);
    assert!(
        matches!(res_sym_file, Err(Error::OutOfScope(_))),
        "expected OutOfScope when target file is a symlink, got {res_sym_file:?}"
    );
    assert_eq!(
        std::fs::read(&existing_outside).unwrap(),
        b"untouched\n",
        "existing outside file must remain untouched when target file is a symlink"
    );
    std::fs::remove_file(&target_existing).unwrap();

    // 2b. Dangling symlink at the final file component inside a real folder.
    let dangling_dest = elsewhere_tmp.path().join("does-not-exist.txt");
    std::os::unix::fs::symlink(&dangling_dest, &target_existing).unwrap();
    let res_dangling_file = write_outside(&allowed, &target_existing, payload, &audited);
    assert!(
        matches!(res_dangling_file, Err(Error::OutOfScope(_))),
        "expected OutOfScope when target file is a dangling symlink, got {res_dangling_file:?}"
    );
    assert!(
        !dangling_dest.exists(),
        "dangling symlink target must not be created"
    );
    std::fs::remove_file(&target_existing).unwrap();

    // 3. TOCTOU swap during `set_write_pause_hook` between `check_outside_write_binding` and `write_atomic`.
    let folder_for_hook = folder.clone();
    let elsewhere_for_hook = elsewhere_tmp.path().to_path_buf();
    set_write_pause_hook(Some(Arc::new(move |_resolved| {
        let _ = std::fs::remove_dir(&folder_for_hook);
        let _ = std::os::unix::fs::symlink(&elsewhere_for_hook, &folder_for_hook);
    })));
    let res_hook_swap = write_outside(&allowed, &target_existing, payload, &audited);
    set_write_pause_hook(None);
    assert!(
        matches!(res_hook_swap, Err(Error::OutOfScope(_))),
        "expected OutOfScope when parent directory is swapped during write pause hook, got {res_hook_swap:?}"
    );
    assert_eq!(
        std::fs::read(&existing_outside).unwrap(),
        b"untouched\n",
        "existing outside file must remain untouched when parent is swapped during pause window"
    );

    // 4. TOCTOU swap during `BeforeMkdir` (ancestor `folder` replaced with a symlink right before creating `new_dir`).
    std::fs::remove_file(&folder).unwrap();
    std::fs::create_dir_all(&folder).unwrap();
    let moved_mkdir = dest_tmp.path().join("folder_moved_mkdir");
    let folder_mkdir = folder.clone();
    let moved_mkdir_cl = moved_mkdir.clone();
    let elsewhere_mkdir = elsewhere_tmp.path().to_path_buf();
    set_atomic_stage_hook(Some(Arc::new(move |stage, _path| {
        if stage == AtomicStage::BeforeMkdir {
            let _ = std::fs::rename(&folder_mkdir, &moved_mkdir_cl);
            let _ = std::os::unix::fs::symlink(&elsewhere_mkdir, &folder_mkdir);
        }
    })));
    let res_mkdir_swap = write_outside(&allowed, &target_nested, payload, &audited);
    set_atomic_stage_hook(None);
    assert!(
        matches!(res_mkdir_swap, Err(Error::OutOfScope(_))),
        "expected OutOfScope when ancestor is swapped before mkdir, got {res_mkdir_swap:?}"
    );
    assert!(
        !elsewhere_tmp.path().join("new_dir").exists(),
        "mkdir must not follow a swapped ancestor symlink into elsewhere"
    );
    let _ = std::fs::remove_file(&folder);
    let _ = std::fs::remove_dir_all(&moved_mkdir);
    std::fs::create_dir_all(&folder).unwrap();

    // 5. TOCTOU swap during `BeforeTmpOpen` (parent `folder` replaced with a symlink after directory validation and before temporary-file open).
    let moved_tmpopen = dest_tmp.path().join("folder_moved_tmpopen");
    let folder_tmpopen = folder.clone();
    let moved_tmpopen_cl = moved_tmpopen.clone();
    let elsewhere_tmpopen = elsewhere_tmp.path().to_path_buf();
    set_atomic_stage_hook(Some(Arc::new(move |stage, _path| {
        if stage == AtomicStage::BeforeTmpOpen {
            let _ = std::fs::rename(&folder_tmpopen, &moved_tmpopen_cl);
            let _ = std::os::unix::fs::symlink(&elsewhere_tmpopen, &folder_tmpopen);
        }
    })));
    let res_tmpopen_swap = write_outside(&allowed, &target_existing, payload, &audited);
    set_atomic_stage_hook(None);
    assert!(
        matches!(res_tmpopen_swap, Err(Error::OutOfScope(_))),
        "expected OutOfScope when parent is swapped before temp open, got {res_tmpopen_swap:?}"
    );
    let outside_entries_after_tmpopen: Vec<String> = std::fs::read_dir(elsewhere_tmp.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        outside_entries_after_tmpopen,
        vec!["existing.txt".to_owned()],
        "temp open must never write a temporary file into the swapped symlink target"
    );
    let _ = std::fs::remove_file(&folder);
    let _ = std::fs::remove_dir_all(&moved_tmpopen);
    std::fs::create_dir_all(&folder).unwrap();

    // 6. TOCTOU swap during `BeforeRename` (parent `folder` replaced with a symlink or target replaced with a dangling symlink right before rename).
    let moved_rename = dest_tmp.path().join("folder_moved_rename");
    let folder_rename = folder.clone();
    let moved_rename_cl = moved_rename.clone();
    let elsewhere_rename = elsewhere_tmp.path().to_path_buf();
    set_atomic_stage_hook(Some(Arc::new(move |stage, _path| {
        if stage == AtomicStage::BeforeRename {
            let _ = std::fs::rename(&folder_rename, &moved_rename_cl);
            let _ = std::os::unix::fs::symlink(&elsewhere_rename, &folder_rename);
        }
    })));
    let res_rename_swap = write_outside(&allowed, &target_existing, payload, &audited);
    set_atomic_stage_hook(None);
    assert!(
        matches!(res_rename_swap, Err(Error::OutOfScope(_))),
        "expected OutOfScope when parent is swapped before rename, got {res_rename_swap:?}"
    );
    assert_eq!(
        std::fs::read(&existing_outside).unwrap(),
        b"untouched\n",
        "existing outside file must remain untouched when parent is swapped before rename"
    );
    let moved_rename_entries: Vec<String> = std::fs::read_dir(&moved_rename)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        moved_rename_entries.is_empty(),
        "owned temporary file in original bound directory must be cleaned up on rename abort, found {moved_rename_entries:?}"
    );
    let _ = std::fs::remove_file(&folder);
    let _ = std::fs::remove_dir_all(&moved_rename);
    std::fs::create_dir_all(&folder).unwrap();

    // 6b. Target replaced with a dangling symlink during `BeforeRename`.
    let target_for_dangling_rename = target_existing.clone();
    let dangling_rename_dest = elsewhere_tmp.path().join("dangling-rename-target.txt");
    let dangling_rename_dest_cl = dangling_rename_dest.clone();
    set_atomic_stage_hook(Some(Arc::new(move |stage, _path| {
        if stage == AtomicStage::BeforeRename {
            let _ =
                std::os::unix::fs::symlink(&dangling_rename_dest_cl, &target_for_dangling_rename);
        }
    })));
    let res_dangling_rename = write_outside(&allowed, &target_existing, payload, &audited);
    set_atomic_stage_hook(None);
    assert!(
        matches!(res_dangling_rename, Err(Error::OutOfScope(_))),
        "expected OutOfScope when target is replaced with a dangling symlink before rename, got {res_dangling_rename:?}"
    );
    assert!(!dangling_rename_dest.exists());
    std::fs::remove_file(&target_existing).unwrap();

    // 6c. Temporary file entry replaced with a foreign regular file during `BeforeRename`.
    let replaced_rename_tmp_holder: Arc<std::sync::Mutex<Option<PathBuf>>> =
        Arc::new(std::sync::Mutex::new(None));
    let replaced_rename_holder_cl = Arc::clone(&replaced_rename_tmp_holder);
    set_atomic_stage_hook(Some(Arc::new(move |stage, hook_path| {
        if stage == AtomicStage::BeforeTmpOpen {
            *replaced_rename_holder_cl.lock().unwrap() = Some(hook_path.to_path_buf());
        } else if stage == AtomicStage::BeforeRename {
            let temp_path = replaced_rename_holder_cl
                .lock()
                .unwrap()
                .clone()
                .expect("BeforeTmpOpen recorded path");
            std::fs::rename(&temp_path, temp_path.with_extension("saved")).unwrap();
            std::fs::write(&temp_path, b"foreign-unapproved-bytes\n").unwrap();
        }
    })));
    let res_replaced_rename = write_outside(&allowed, &target_existing, payload, &audited);
    set_atomic_stage_hook(None);
    assert!(
        matches!(res_replaced_rename, Err(Error::OutOfScope(_))),
        "expected OutOfScope when temporary entry is replaced with foreign file before rename, got {res_replaced_rename:?}"
    );
    assert!(
        !target_existing.exists(),
        "foreign temporary replacement must never be published to target"
    );
    let replaced_rename_tmp = replaced_rename_tmp_holder
        .lock()
        .unwrap()
        .clone()
        .expect("BeforeTmpOpen recorded path");
    assert_eq!(
        std::fs::read(&replaced_rename_tmp).unwrap(),
        b"foreign-unapproved-bytes\n",
        "cleanup after aborted rename must preserve the foreign replacement file at the temp path"
    );
    let _ = std::fs::remove_file(&replaced_rename_tmp);
    let _ = std::fs::remove_file(replaced_rename_tmp.with_extension("saved"));

    // 7. Medium finding: pre-existing temporary file (`AlreadyExists` on `create_new(true)`) must NOT be unlinked.
    let preexisting_tmp_holder: Arc<std::sync::Mutex<Option<PathBuf>>> =
        Arc::new(std::sync::Mutex::new(None));
    let holder_cl = Arc::clone(&preexisting_tmp_holder);
    set_atomic_stage_hook(Some(Arc::new(move |stage, tmp_path| {
        if stage == AtomicStage::BeforeTmpOpen {
            std::fs::write(tmp_path, b"foreign-temp-do-not-unlink\n").unwrap();
            *holder_cl.lock().unwrap() = Some(tmp_path.to_path_buf());
        }
    })));
    let res_already_exists = write_outside(&allowed, &target_existing, payload, &audited);
    set_atomic_stage_hook(None);
    assert!(
        matches!(res_already_exists, Err(Error::Io { .. })),
        "expected Io error when temporary file already exists, got {res_already_exists:?}"
    );
    let foreign_tmp = preexisting_tmp_holder
        .lock()
        .unwrap()
        .clone()
        .expect("BeforeTmpOpen recorded path");
    assert_eq!(
        std::fs::read(&foreign_tmp).unwrap(),
        b"foreign-temp-do-not-unlink\n",
        "cleanup must never unlink a pre-existing file that create_new(true) did not create"
    );
    std::fs::remove_file(&foreign_tmp).unwrap();

    // 8. Medium finding: cleanup on failure after parent swap must unlink only in the bound parent dirfd, never following mutable ancestor paths.
    let moved_cleanup = dest_tmp.path().join("folder_moved_cleanup");
    let folder_cleanup = folder.clone();
    let moved_cleanup_cl = moved_cleanup.clone();
    let elsewhere_cleanup = elsewhere_tmp.path().to_path_buf();
    let target_cleanup = target_existing.clone();
    let canary_holder: Arc<std::sync::Mutex<Option<PathBuf>>> =
        Arc::new(std::sync::Mutex::new(None));
    let canary_cl = Arc::clone(&canary_holder);
    set_atomic_stage_hook(Some(Arc::new(move |stage, hook_path| {
        if stage == AtomicStage::BeforeRename {
            // Place a symlink at target so post-write validation fails and triggers cleanup.
            let _ =
                std::os::unix::fs::symlink(elsewhere_cleanup.join("existing.txt"), &target_cleanup);
        } else if stage == AtomicStage::BeforeCleanup {
            let tmp_name = hook_path.file_name().unwrap();
            let canary = elsewhere_cleanup.join(tmp_name);
            std::fs::write(&canary, b"outside-canary-must-survive\n").unwrap();
            *canary_cl.lock().unwrap() = Some(canary);
            let _ = std::fs::rename(&folder_cleanup, &moved_cleanup_cl);
            let _ = std::os::unix::fs::symlink(&elsewhere_cleanup, &folder_cleanup);
        }
    })));
    let res_cleanup_swap = write_outside(&allowed, &target_existing, payload, &audited);
    set_atomic_stage_hook(None);
    assert!(
        matches!(res_cleanup_swap, Err(Error::OutOfScope(_))),
        "expected OutOfScope when target is swapped before rename, got {res_cleanup_swap:?}"
    );
    let canary_path = canary_holder
        .lock()
        .unwrap()
        .clone()
        .expect("BeforeCleanup recorded canary");
    assert_eq!(
        std::fs::read(&canary_path).unwrap(),
        b"outside-canary-must-survive\n",
        "cleanup must unlink inside the bound directory descriptor and never follow a swapped ancestor symlink"
    );
    std::fs::remove_file(&canary_path).unwrap();
    let _ = std::fs::remove_file(&folder);
    let _ = std::fs::remove_dir_all(&moved_cleanup);
    std::fs::create_dir_all(&folder).unwrap();

    // 8b. Temporary file entry replaced with a foreign regular file in the same bound directory during `BeforeCleanup`.
    let target_cleanup_replace = target_existing.clone();
    let replaced_cleanup_holder: Arc<std::sync::Mutex<Option<PathBuf>>> =
        Arc::new(std::sync::Mutex::new(None));
    let replaced_cleanup_cl = Arc::clone(&replaced_cleanup_holder);
    set_atomic_stage_hook(Some(Arc::new(move |stage, hook_path| {
        if stage == AtomicStage::BeforeRename {
            std::fs::create_dir(&target_cleanup_replace).unwrap();
        } else if stage == AtomicStage::BeforeCleanup {
            std::fs::rename(hook_path, hook_path.with_extension("saved")).unwrap();
            std::fs::write(hook_path, b"foreign-cleanup-replacement-must-survive\n").unwrap();
            *replaced_cleanup_cl.lock().unwrap() = Some(hook_path.to_path_buf());
        }
    })));
    let res_cleanup_replace = write_outside(&allowed, &target_existing, payload, &audited);
    set_atomic_stage_hook(None);
    assert!(
        res_cleanup_replace.is_err(),
        "expected error when destination is a directory at rename, got {res_cleanup_replace:?}"
    );
    let replaced_cleanup_tmp = replaced_cleanup_holder
        .lock()
        .unwrap()
        .clone()
        .expect("BeforeCleanup recorded replacement path");
    assert_eq!(
        std::fs::read(&replaced_cleanup_tmp).unwrap(),
        b"foreign-cleanup-replacement-must-survive\n",
        "cleanup must preserve a foreign replacement file placed at the temporary path"
    );
    let _ = std::fs::remove_file(&replaced_cleanup_tmp);
    let _ = std::fs::remove_file(replaced_cleanup_tmp.with_extension("saved"));
    let _ = std::fs::remove_dir(&target_existing);

    // 8c. A foreign sibling written in the same bound directory during cleanup survives it.
    // The unlink-boundary race is covered by `cleanup_unlink_race_never_forges_a_copy`.
    let target_cleanup_sibling = target_existing.clone();
    let cleanup_tmp_holder: Arc<std::sync::Mutex<Option<PathBuf>>> =
        Arc::new(std::sync::Mutex::new(None));
    let cleanup_tmp_cl = Arc::clone(&cleanup_tmp_holder);
    set_atomic_stage_hook(Some(Arc::new(move |stage, hook_path| {
        if stage == AtomicStage::BeforeTmpOpen {
            *cleanup_tmp_cl.lock().unwrap() = Some(hook_path.to_path_buf());
        } else if stage == AtomicStage::BeforeRename {
            std::fs::create_dir(&target_cleanup_sibling).unwrap();
        } else if stage == AtomicStage::BeforeCleanup {
            std::fs::write(
                hook_path.parent().unwrap().join(".review-foreign"),
                b"foreign-preserve",
            )
            .unwrap();
        }
    })));
    let res_cleanup_sibling = write_outside(&allowed, &target_existing, payload, &audited);
    set_atomic_stage_hook(None);
    assert!(
        res_cleanup_sibling.is_err(),
        "expected error when destination is a directory at rename, got {res_cleanup_sibling:?}"
    );
    let cleanup_tmp_path = cleanup_tmp_holder
        .lock()
        .unwrap()
        .clone()
        .expect("BeforeTmpOpen recorded path");
    let review_foreign_path = folder.join(".review-foreign");
    let preserved_bytes =
        std::fs::read(&review_foreign_path).expect("foreign sibling must survive cleanup");
    assert_eq!(preserved_bytes, b"foreign-preserve");
    assert!(
        std::fs::symlink_metadata(&cleanup_tmp_path).is_err(),
        "cleanup must unlink the verified owned temporary entry"
    );
    let _ = std::fs::remove_file(&cleanup_tmp_path);
    let _ = std::fs::remove_file(&review_foreign_path);
    let _ = std::fs::remove_dir(&target_existing);

    // 9. Restoring `folder` as a real directory allows the bound write to succeed.
    let sha = write_outside(&allowed, &target_existing, payload, &audited)
        .expect("legitimate bound write_outside must succeed once folder is a real directory");
    assert_eq!(sha, payload_sha);
    assert_eq!(std::fs::read(&target_existing).unwrap(), payload);
    assert_eq!(std::fs::read(&existing_outside).unwrap(), b"untouched\n");
}

/// Cleanup that loses a race at the unlink system-call boundary must not forge recovery.
///
/// POSIX has no unlink-by-identity call, so a replacement that lands after the cleanup
/// identity check and before `unlink` cannot be stopped from user space. What cleanup must
/// never do is fabricate a byte copy of the lost file: that copy is a new inode with new
/// ownership, mode and timestamps, and it hides the loss. The entry at the temporary path is
/// either gone or still the original inode, never a recreated stand-in.
#[cfg(unix)]
#[test]
fn cleanup_unlink_race_never_forges_a_copy() {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

    let dest_tmp = tempfile::tempdir().unwrap();
    let folder = dest_tmp.path().join("folder");
    std::fs::create_dir_all(&folder).unwrap();
    let target = folder.join("existing.txt");
    let payload = b"payload\n";
    let payload_sha = Sha256::digest(payload);

    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    let delivery = Delivery {
        entries: vec![Transfer::write_path(target.clone(), payload)],
        markings: None,
    };
    let Decision::Allowed(allowed) = check(
        &floor_a_effective(),
        &Dest::Export {
            path: dest_tmp.path().to_string_lossy().into_owned(),
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
        files: vec![WriteFile::new(
            "folder/existing.txt",
            payload_sha.as_str().to_owned(),
        )],
        affected: vec![RecordRef::parse("corpus/S-0001").unwrap_or_else(|e| panic!("{e}"))],
    };
    let mut journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let audited = journal
        .begin_export("context", &ws, &allowed, dest_tmp.path())
        .unwrap_or_else(|e| panic!("{e}"));

    let foreign = folder.join("foreign.dat");
    let aside = folder.join("owned.aside");
    let tmp_holder: Arc<std::sync::Mutex<Option<PathBuf>>> = Arc::new(std::sync::Mutex::new(None));
    let foreign_ino: Arc<std::sync::Mutex<Option<u64>>> = Arc::new(std::sync::Mutex::new(None));
    let (tmp_cl, ino_cl, foreign_cl, aside_cl, target_cl) = (
        Arc::clone(&tmp_holder),
        Arc::clone(&foreign_ino),
        foreign.clone(),
        aside.clone(),
        target.clone(),
    );
    set_atomic_stage_hook(Some(Arc::new(move |stage, hook_path| match stage {
        AtomicStage::BeforeTmpOpen => *tmp_cl.lock().unwrap() = Some(hook_path.to_path_buf()),
        // A directory at the target makes the rename fail, so the guard runs cleanup.
        AtomicStage::BeforeRename => std::fs::create_dir(&target_cl).unwrap(),
        // A foreign sibling exists while cleanup verifies the temporary entry.
        AtomicStage::BeforeCleanup => {
            std::fs::write(&foreign_cl, b"foreign-bytes\n").unwrap();
            std::fs::set_permissions(&foreign_cl, std::fs::Permissions::from_mode(0o640)).unwrap();
            *ino_cl.lock().unwrap() = Some(std::fs::metadata(&foreign_cl).unwrap().ino());
        }
        // After verification, the owned entry is moved aside and the sibling takes its name.
        AtomicStage::CleanupUnlink => {
            std::fs::rename(hook_path, &aside_cl).unwrap();
            std::fs::rename(&foreign_cl, hook_path).unwrap();
        }
        _ => {}
    })));
    let res = write_outside(&allowed, &target, payload, &audited);
    set_atomic_stage_hook(None);
    assert!(
        res.is_err(),
        "rename onto a directory must fail, got {res:?}"
    );

    let tmp_path = tmp_holder
        .lock()
        .unwrap()
        .clone()
        .expect("temporary path recorded");
    let original_ino = foreign_ino.lock().unwrap().expect("foreign inode recorded");
    if let Ok(meta) = std::fs::symlink_metadata(&tmp_path) {
        assert_eq!(
            meta.ino(),
            original_ino,
            "cleanup recreated a byte copy at the temporary path instead of leaving the original inode"
        );
    }
    assert!(
        !foreign.exists(),
        "the foreign file was moved onto the temporary path by the race"
    );
    let _ = std::fs::remove_file(&tmp_path);
    let _ = std::fs::remove_file(&aside);
    let _ = std::fs::remove_dir(&target);
}

#[cfg(windows)]
#[test]
fn write_outside_rejects_post_binding_junction_swap() {
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());

    let dest_tmp = tempfile::tempdir().unwrap();
    let folder = dest_tmp.path().join("folder");
    std::fs::create_dir_all(&folder).unwrap();

    let elsewhere_tmp = tempfile::tempdir().unwrap();
    let existing_outside = elsewhere_tmp.path().join("existing.txt");
    std::fs::write(&existing_outside, b"untouched\n").unwrap();

    let target_existing = folder.join("existing.txt");
    let target_nested = folder.join("new_dir/escaped.txt");
    let payload = b"overwritten\n";
    let payload_sha = Sha256::digest(payload);

    let delivery = Delivery {
        entries: vec![
            Transfer::write_path(target_existing.clone(), payload),
            Transfer::write_path(target_nested.clone(), payload),
        ],
        markings: None,
    };
    let Decision::Allowed(allowed) = check(
        &floor_a_effective(),
        &Dest::Export {
            path: dest_tmp.path().to_string_lossy().into_owned(),
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
        files: vec![
            WriteFile::new("folder/existing.txt", payload_sha.as_str().to_owned()),
            WriteFile::new(
                "folder/new_dir/escaped.txt",
                payload_sha.as_str().to_owned(),
            ),
        ],
        affected: vec![RecordRef::parse("corpus/S-0001").unwrap_or_else(|e| panic!("{e}"))],
    };
    let mut journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let audited = journal
        .begin_export("context", &ws, &allowed, dest_tmp.path())
        .unwrap_or_else(|e| panic!("{e}"));

    // 1. Replace `folder` with a Windows directory junction (`mklink /J`) to `elsewhere_tmp`.
    std::fs::remove_dir(&folder).unwrap();
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&folder)
        .arg(elsewhere_tmp.path())
        .status()
        .expect("cmd /C mklink /J starts");
    assert!(status.success(), "mklink /J succeeded");

    let res_junction = write_outside(&allowed, &target_existing, payload, &audited);
    assert!(
        matches!(res_junction, Err(Error::OutOfScope(_))),
        "expected OutOfScope when parent directory is a Windows junction, got {res_junction:?}"
    );
    assert_eq!(
        std::fs::read(&existing_outside).unwrap(),
        b"untouched\n",
        "existing outside file must remain untouched when parent is a junction"
    );

    let res_nested_junction = write_outside(&allowed, &target_nested, payload, &audited);
    assert!(
        matches!(res_nested_junction, Err(Error::OutOfScope(_))),
        "expected OutOfScope for nested path under a Windows junction, got {res_nested_junction:?}"
    );
    assert!(!elsewhere_tmp.path().join("new_dir").exists());

    // 2. Restore real directory and verify pre-existing temp file is not unlinked on AlreadyExists.
    std::fs::remove_dir(&folder).unwrap();
    std::fs::create_dir_all(&folder).unwrap();
    let preexisting_tmp_holder: Arc<std::sync::Mutex<Option<PathBuf>>> =
        Arc::new(std::sync::Mutex::new(None));
    let holder_cl = Arc::clone(&preexisting_tmp_holder);
    set_atomic_stage_hook(Some(Arc::new(move |stage, tmp_path| {
        if stage == AtomicStage::BeforeTmpOpen {
            std::fs::write(tmp_path, b"foreign-temp-do-not-unlink\n").unwrap();
            *holder_cl.lock().unwrap() = Some(tmp_path.to_path_buf());
        }
    })));
    let res_already_exists = write_outside(&allowed, &target_existing, payload, &audited);
    set_atomic_stage_hook(None);
    assert!(matches!(res_already_exists, Err(Error::Io { .. })));
    let foreign_tmp = preexisting_tmp_holder
        .lock()
        .unwrap()
        .clone()
        .expect("BeforeTmpOpen recorded path");
    assert_eq!(
        std::fs::read(&foreign_tmp).unwrap(),
        b"foreign-temp-do-not-unlink\n"
    );
    std::fs::remove_file(&foreign_tmp).unwrap();

    // 3. Temporary entry replaced with a foreign file during `BeforeRename` must be refused.
    let replaced_rename_tmp_holder: Arc<std::sync::Mutex<Option<PathBuf>>> =
        Arc::new(std::sync::Mutex::new(None));
    let replaced_rename_cl = Arc::clone(&replaced_rename_tmp_holder);
    set_atomic_stage_hook(Some(Arc::new(move |stage, hook_path| {
        if stage == AtomicStage::BeforeTmpOpen {
            *replaced_rename_cl.lock().unwrap() = Some(hook_path.to_path_buf());
        } else if stage == AtomicStage::BeforeRename {
            let temp_path = replaced_rename_cl
                .lock()
                .unwrap()
                .clone()
                .expect("BeforeTmpOpen recorded path");
            std::fs::rename(&temp_path, temp_path.with_extension("saved")).unwrap();
            std::fs::write(&temp_path, b"foreign-unapproved-bytes\n").unwrap();
        }
    })));
    let res_replaced_rename = write_outside(&allowed, &target_existing, payload, &audited);
    set_atomic_stage_hook(None);
    assert!(matches!(res_replaced_rename, Err(Error::OutOfScope(_))));
    assert!(!target_existing.exists());
    let replaced_rename_tmp = replaced_rename_tmp_holder
        .lock()
        .unwrap()
        .clone()
        .expect("BeforeTmpOpen recorded path");
    assert_eq!(
        std::fs::read(&replaced_rename_tmp).unwrap(),
        b"foreign-unapproved-bytes\n"
    );
    let _ = std::fs::remove_file(&replaced_rename_tmp);
    let _ = std::fs::remove_file(replaced_rename_tmp.with_extension("saved"));

    // 4. Temporary entry replaced with a foreign file during `BeforeCleanup` must be preserved.
    let target_cleanup_replace = target_existing.clone();
    let replaced_cleanup_holder: Arc<std::sync::Mutex<Option<PathBuf>>> =
        Arc::new(std::sync::Mutex::new(None));
    let replaced_cleanup_cl = Arc::clone(&replaced_cleanup_holder);
    set_atomic_stage_hook(Some(Arc::new(move |stage, hook_path| {
        if stage == AtomicStage::BeforeRename {
            std::fs::create_dir(&target_cleanup_replace).unwrap();
        } else if stage == AtomicStage::BeforeCleanup {
            std::fs::rename(hook_path, hook_path.with_extension("saved")).unwrap();
            std::fs::write(hook_path, b"foreign-cleanup-replacement-must-survive\n").unwrap();
            *replaced_cleanup_cl.lock().unwrap() = Some(hook_path.to_path_buf());
        }
    })));
    let res_cleanup_replace = write_outside(&allowed, &target_existing, payload, &audited);
    set_atomic_stage_hook(None);
    assert!(res_cleanup_replace.is_err());
    let replaced_cleanup_tmp = replaced_cleanup_holder
        .lock()
        .unwrap()
        .clone()
        .expect("BeforeCleanup recorded replacement path");
    assert_eq!(
        std::fs::read(&replaced_cleanup_tmp).unwrap(),
        b"foreign-cleanup-replacement-must-survive\n"
    );
    let _ = std::fs::remove_file(&replaced_cleanup_tmp);
    let _ = std::fs::remove_file(replaced_cleanup_tmp.with_extension("saved"));
    let _ = std::fs::remove_dir(&target_existing);

    let sha = write_outside(&allowed, &target_existing, payload, &audited)
        .expect("legitimate bound write_outside must succeed once folder is a real directory");
    assert_eq!(sha, payload_sha);
}

// A destination whose ancestor grants traversal but not read: the component walk must need
// execute permission only, and must not fail with PermissionDenied on the ancestor.
#[cfg(unix)]
#[test]
fn write_outside_allows_an_execute_only_ancestor() {
    use std::os::unix::fs::PermissionsExt as _;

    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());

    let holder = tempfile::tempdir().unwrap();
    let ancestor = holder.path().join("traversal-only");
    let dest = ancestor.join("dest");
    std::fs::create_dir_all(&dest).unwrap();
    std::fs::set_permissions(&ancestor, std::fs::Permissions::from_mode(0o111)).unwrap();

    let target = dest.join("existing.txt");
    let payload = b"written\n";
    let delivery = Delivery {
        entries: vec![Transfer::write_path(target.clone(), payload)],
        markings: None,
    };
    let Decision::Allowed(allowed) = check(
        &floor_a_effective(),
        &Dest::Export {
            path: dest.to_string_lossy().into_owned(),
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
        files: vec![WriteFile::new(
            "existing.txt",
            Sha256::digest(payload).as_str().to_owned(),
        )],
        affected: vec![RecordRef::parse("corpus/S-0001").unwrap_or_else(|e| panic!("{e}"))],
    };
    let mut journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let audited = journal
        .begin_export("context", &ws, &allowed, &dest)
        .unwrap_or_else(|e| panic!("{e}"));

    let result = write_outside(&allowed, &target, payload, &audited);
    // Restore the mode before asserting, so the holder can be removed whichever way the write went.
    std::fs::set_permissions(&ancestor, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(
        result.unwrap_or_else(|e| panic!("write beneath an execute-only ancestor: {e}")),
        Sha256::digest(payload)
    );
    assert_eq!(std::fs::read(&target).unwrap(), payload);
}
