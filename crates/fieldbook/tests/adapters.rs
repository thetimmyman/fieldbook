//! Integration and contract tests for port adapters and adapter registry drift (`AD-01`, `ADR 0004 §11`).

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::print_stdout,
    reason = "integration tests assert adapter contract conformance and drift invariants"
)]

mod support;

#[path = "../../../tests/support/extractor_suite.rs"]
mod extractor_suite;

// Included here so that the registered suites need no line in `support/mod.rs`.
#[path = "support/authz_suite.rs"]
mod authz_suite;
#[expect(
    clippy::duplicate_mod,
    reason = "shared seeded test RNG is included per integration test binary"
)]
#[path = "../../../tests/support/rng.rs"]
mod rng;
#[path = "support/store_suite.rs"]
mod store_suite;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use fieldbook::adapters::extract_io::Staged;
use fieldbook::adapters::registry::{ADAPTERS, Caps, Port, Row, State};
use fieldbook::io::Disk;
use fieldbook_core::audit::{Actor, Principal, Role, Sha256, Via};
use fieldbook_core::authz::{Act, Authz, AuthzDecision, PolicyReaders, Scope};
use fieldbook_core::extraction::methods::{METHODS, Method};
use fieldbook_core::extraction::{
    ExtractError, Extraction, Extractor, HtmlStrip, PageText, Plain, build_record,
    thin_page_numbers,
};
use fieldbook_core::hash::sha256_hex;
use fieldbook_core::labels::StoreId;
use fieldbook_core::pipeline::VerifiedMethodMetadata;
use fieldbook_core::policy::Policy;

struct FolderTargetFactory;

impl support::backup_suite::BackupFactory for FolderTargetFactory {
    fn make<'a>(
        &self,
        world: &'a support::world::TestWorld,
        audited: &'a fieldbook::io::Audited,
        allowed: &'a fieldbook_core::egress::Allowed,
        target: &'a Path,
    ) -> Box<dyn fieldbook_core::backup::BackupTarget + 'a> {
        Box::new(fieldbook::io::backup_folder::FolderTarget::new(
            world, audited, allowed, target,
        ))
    }
}

fn sample_actor() -> Actor {
    Actor {
        principal: Principal::local(),
        role: Role::Owner,
        via: Via::Cli,
    }
}

fn make_policy_readers<'a>(
    stores: &[(StoreId, Option<&str>, Option<&str>)],
    groups: &'a dyn fieldbook_core::authz::Groups,
) -> Box<dyn Authz + 'a> {
    let mut policy_text = String::from("policy_schema = 1\nrevision = 1\n");
    for &(store, readers, changers) in stores {
        if store == StoreId::Corpus {
            continue;
        }
        policy_text.push_str("\n[[store]]\n");
        policy_text.push_str(&format!("id = \"{}\"\n", store.as_str()));
        policy_text.push_str("holds = [\"public\", \"unlabelled\"]\n");
        if let Some(r) = readers {
            policy_text.push_str(&format!("readers = \"{r}\"\n"));
        }
        if let Some(c) = changers {
            policy_text.push_str(&format!("changers = \"{c}\"\n"));
        }
    }
    let policy = Policy::parse(&policy_text, None).expect("valid policy text");
    Box::new(PolicyReaders::new(&policy, groups))
}

#[test]
fn contract_extractor_plain_1() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fix_dir = repo_root.join("tests/fixtures/ports/extractor/plain@1");
    let input = std::fs::read(fix_dir.join("input.txt")).expect("read input.txt");
    let expect_toml =
        std::fs::read_to_string(fix_dir.join("expect.toml")).expect("read expect.toml");
    let expect: toml::Table = toml::from_str(&expect_toml).expect("parse expect.toml");

    let expected_method = expect
        .get("method")
        .and_then(toml::Value::as_str)
        .expect("method");
    let expected_pages = usize::try_from(
        expect
            .get("pages")
            .and_then(toml::Value::as_integer)
            .expect("pages"),
    )
    .expect("positive pages");

    let extractor = Plain;
    let extraction = extractor.extract(&input, "txt").expect("extract plain");
    assert_eq!(extraction.method, expected_method);
    assert_eq!(extraction.pages.len(), expected_pages);
    assert_eq!(thin_page_numbers(&extraction.pages), Vec::<u32>::new());

    let vectors = extractor_suite::text_vectors();
    extractor_suite::extractor_contract(&extractor, "txt", &vectors, &|ext| {
        if ext.method == fieldbook_core::extraction::methods::PLAIN {
            Ok(())
        } else {
            Err(format!(
                "expected method {}, got {}",
                fieldbook_core::extraction::methods::PLAIN,
                ext.method
            ))
        }
    });
}

#[test]
fn contract_extractor_html_strip_1() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fix_dir = repo_root.join("tests/fixtures/ports/extractor/html-strip@1");
    let input = std::fs::read(fix_dir.join("input.html")).expect("read input.html");
    let expect_toml =
        std::fs::read_to_string(fix_dir.join("expect.toml")).expect("read expect.toml");
    let expect: toml::Table = toml::from_str(&expect_toml).expect("parse expect.toml");

    let expected_method = expect
        .get("method")
        .and_then(toml::Value::as_str)
        .expect("method");
    let expected_pages = usize::try_from(
        expect
            .get("pages")
            .and_then(toml::Value::as_integer)
            .expect("pages"),
    )
    .expect("positive pages");

    let extractor = HtmlStrip;
    let extraction = extractor
        .extract(&input, "html")
        .expect("extract html-strip");
    assert_eq!(extraction.method, expected_method);
    assert_eq!(extraction.pages.len(), expected_pages);
    assert_eq!(thin_page_numbers(&extraction.pages), Vec::<u32>::new());

    let vectors = extractor_suite::text_vectors();
    extractor_suite::extractor_contract(&extractor, "html", &vectors, &|ext| {
        if ext.method == fieldbook_core::extraction::methods::HTML_STRIP {
            Ok(())
        } else {
            Err(format!(
                "expected method {}, got {}",
                fieldbook_core::extraction::methods::HTML_STRIP,
                ext.method
            ))
        }
    });
}

fn copy_state_intake_into(root: &Path) -> PathBuf {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let src_state = repo_root.join("tests/fixtures/state-intake");
    let dst_state = root.join("state");
    fn copy_dir(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).expect("create_dir_all");
        for entry in std::fs::read_dir(from).expect("read_dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            let target = to.join(entry.file_name());
            if path.is_dir() {
                copy_dir(&path, &target);
            } else {
                std::fs::copy(&path, &target).expect("copy file");
            }
        }
    }
    copy_dir(&src_state, &dst_state);
    dst_state
}

#[test]
fn contract_extractor_staged_1() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fix_dir = repo_root.join("tests/fixtures/ports/extractor/staged@1");
    let input = std::fs::read(fix_dir.join("input.txt")).expect("read input.txt");
    let expect_toml =
        std::fs::read_to_string(fix_dir.join("expect.toml")).expect("read expect.toml");
    let expect: toml::Table = toml::from_str(&expect_toml).expect("parse expect.toml");
    let expected_method = expect
        .get("method")
        .and_then(toml::Value::as_str)
        .expect("method");
    let expected_pages = usize::try_from(
        expect
            .get("pages")
            .and_then(toml::Value::as_integer)
            .expect("pages"),
    )
    .expect("positive pages");

    let tmp = tempfile::tempdir().expect("create tempdir");
    let state_dir = copy_state_intake_into(tmp.path());
    let world = support::world::TestWorld::with_state(tmp.path(), &state_dir);
    let extractor = Staged::new(&world);
    assert_eq!(extractor.id(), "staged@1");

    let extraction = extractor.extract(&input, "txt").expect("replay staged");
    assert_eq!(extraction.method, expected_method);
    assert_eq!(extraction.pages.len(), expected_pages);
    assert_eq!(thin_page_numbers(&extraction.pages), Vec::<u32>::new());

    // Populate staged entries for all valid contract text vectors in this isolated TempDir,
    // and expect `Missing` for any vector without a staged folder.
    let mut vectors = extractor_suite::text_vectors();
    for vec in &mut vectors {
        if let Ok(ext) = Plain.extract(&vec.input, "txt") {
            let sha = sha256_hex(&vec.input);
            let record = build_record(&ext, "plain", "1", &sha, &BTreeMap::new());
            let dir = state_dir.join("staging").join(&sha);
            std::fs::create_dir_all(&dir).expect("create staging dir");
            std::fs::write(dir.join("extraction.toml"), record.render()).expect("write toml");
            for page in &ext.pages {
                let stem = fieldbook_core::extraction::page_stem(page.number);
                std::fs::write(dir.join(format!("{stem}.txt")), page.text.as_bytes())
                    .expect("write page");
            }
        } else {
            vec.expect = extractor_suite::Expect::Refused("Missing");
        }
    }
    extractor_suite::extractor_contract(&extractor, "txt", &vectors, &|ext| {
        if ext.method == fieldbook_core::extraction::methods::PLAIN {
            Ok(())
        } else {
            Err(format!("unexpected method {}", ext.method))
        }
    });

    // Unknown bytes without a staging folder return ExtractError::Missing("staging folder").
    assert_eq!(
        extractor.extract(b"unstaged content", "txt"),
        Err(ExtractError::Missing("staging folder"))
    );
}

#[test]
fn staged_rejects_a_changed_page() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let input = std::fs::read(repo_root.join("tests/fixtures/ports/extractor/staged@1/input.txt"))
        .expect("read input.txt");
    let orig_sha = sha256_hex(&input);

    // Mutate one byte in staged p0001.txt.
    let tmp = tempfile::tempdir().expect("create tempdir");
    let state_dir = copy_state_intake_into(tmp.path());
    let page_path = state_dir.join("staging").join(&orig_sha).join("p0001.txt");
    let mut page_bytes = std::fs::read(&page_path).expect("read p0001.txt");
    page_bytes[0] ^= 0x01;
    std::fs::write(&page_path, &page_bytes).expect("write mutated page");

    let world = support::world::TestWorld::with_state(tmp.path(), &state_dir);
    let extractor = Staged::new(&world);
    let err = extractor
        .extract(&input, "txt")
        .expect_err("changed page must fail");
    assert_eq!(err, ExtractError::Missing("matching staging folder"));
    let line = format!("{}: {err}", extractor.id());
    assert!(
        line.starts_with("staged@1"),
        "printed refusal line must start with `staged@1`: {line}"
    );

    // Restoring p0001.txt succeeds, while mutating original input returns Missing("staging folder").
    let mut mutated_input = input;
    mutated_input[0] ^= 0x01;
    assert_eq!(
        extractor.extract(&mutated_input, "txt"),
        Err(ExtractError::Missing("staging folder"))
    );
}

#[test]
fn staged_replays_known_and_preserves_unsupported_metadata_as_unavailable() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let state_dir = tmp.path().join("state");
    let orig_bytes = b"%PDF-1.4\nsynthetic fixture payload\n";
    let orig_sha = sha256_hex(orig_bytes);
    let staging_dir = state_dir.join("staging").join(&orig_sha);
    std::fs::create_dir_all(&staging_dir).expect("create staging dir");

    let long_page =
        "Synthetic extracted PDF body text long enough to avoid being flagged thin in tests.\n"
            .to_owned();
    let p1_sha = sha256_hex(long_page.as_bytes());
    std::fs::write(staging_dir.join("p0001.txt"), long_page.as_bytes()).expect("write p0001");

    // 1. Known method (`pdftotext-layout`) preserves validated tool metadata without re-running PDF tool.
    let known_ext = Extraction {
        method: fieldbook_core::extraction::methods::PDFTOTEXT_LAYOUT,
        pages: vec![PageText {
            number: 1,
            text: long_page.clone(),
        }],
    };
    let record = build_record(
        &known_ext,
        "pdftotext",
        "24.02.0",
        &orig_sha,
        &BTreeMap::new(),
    );
    std::fs::write(staging_dir.join("extraction.toml"), record.render()).expect("write known toml");

    let world = support::world::TestWorld::with_state(tmp.path(), &state_dir);
    let staged = Staged::new(&world);
    let verified = staged
        .verify(orig_bytes)
        .expect("verify known pdftotext-layout");
    assert_eq!(
        verified.extraction().method,
        fieldbook_core::extraction::methods::PDFTOTEXT_LAYOUT
    );
    assert_eq!(verified.tool(), "pdftotext");
    assert_eq!(verified.tool_version(), "24.02.0");
    assert!(matches!(
        verified.method_metadata(),
        VerifiedMethodMetadata::Known { .. }
    ));

    // 2. Unsupported method preserves metadata as Unavailable rather than silently inventing Known fields.
    let _ = p1_sha;
    let unsupported_toml = record.render().replace(
        "method = \"pdftotext-layout\"",
        "method = \"future-ocr-engine\"",
    );
    std::fs::write(staging_dir.join("extraction.toml"), &unsupported_toml)
        .expect("write unsupported toml");
    let verified_unsup = staged
        .verify(orig_bytes)
        .expect("verify unsupported staging preserves hash-checked pages");
    assert_eq!(verified_unsup.extraction().method, "unsupported");
    assert_eq!(
        verified_unsup.method_metadata(),
        &VerifiedMethodMetadata::Unavailable {
            method: "future-ocr-engine".to_owned(),
            reason: "unsupported extraction method",
        }
    );
}

#[cfg(unix)]
#[test]
fn staged_refuses_a_symlinked_page_file() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let input = std::fs::read(repo_root.join("tests/fixtures/ports/extractor/staged@1/input.txt"))
        .expect("read input.txt");
    let orig_sha = sha256_hex(&input);

    let tmp = tempfile::tempdir().expect("create tempdir");
    let state_dir = copy_state_intake_into(tmp.path());
    let page_path = state_dir.join("staging").join(&orig_sha).join("p0001.txt");
    let outside_target = tmp.path().join("outside_page.txt");
    let page_bytes = std::fs::read(&page_path).expect("read p0001.txt");
    std::fs::write(&outside_target, &page_bytes).expect("write outside file");
    std::fs::remove_file(&page_path).expect("remove original p0001.txt");
    std::os::unix::fs::symlink(&outside_target, &page_path).expect("plant symlink");

    let world = support::world::TestWorld::with_state(tmp.path(), &state_dir);
    let extractor = Staged::new(&world);
    assert_eq!(
        extractor.extract(&input, "txt"),
        Err(ExtractError::Missing("staging folder"))
    );
}

#[cfg(unix)]
#[test]
fn staged_refuses_symlinked_staging_ancestors() {
    use fieldbook::World as _;
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let input = std::fs::read(repo_root.join("tests/fixtures/ports/extractor/staged@1/input.txt"))
        .expect("read input.txt");
    let orig_sha = sha256_hex(&input);

    // 1. Symlink at `<state>/staging` pointing outside `<state>`.
    let tmp1 = tempfile::tempdir().expect("create tempdir 1");
    let state1 = copy_state_intake_into(tmp1.path());
    let outside_staging = tmp1.path().join("outside_staging");
    std::fs::rename(state1.join("staging"), &outside_staging).expect("move staging outside");
    std::os::unix::fs::symlink(&outside_staging, state1.join("staging"))
        .expect("symlink staging dir");
    let world1 = support::world::TestWorld::with_state(tmp1.path(), &state1);
    assert_eq!(
        Staged::new(&world1).extract(&input, "txt"),
        Err(ExtractError::Missing("staging folder")),
        "symlinked `<state>/staging` directory must be refused"
    );

    // 2. Symlink at `<state>/staging/<sha256>` pointing outside `staging`.
    let tmp2 = tempfile::tempdir().expect("create tempdir 2");
    let state2 = copy_state_intake_into(tmp2.path());
    let sha_dir = state2.join("staging").join(&orig_sha);
    let outside_sha = tmp2.path().join("outside_sha");
    std::fs::rename(&sha_dir, &outside_sha).expect("move <sha256> outside");
    std::os::unix::fs::symlink(&outside_sha, &sha_dir).expect("symlink <sha256> dir");
    let world2 = support::world::TestWorld::with_state(tmp2.path(), &state2);
    assert_eq!(
        Staged::new(&world2).extract(&input, "txt"),
        Err(ExtractError::Missing("staging folder")),
        "symlinked `<state>/staging/<sha256>` directory must be refused"
    );

    // 3. `..` traversal component in `World::read_file` must be refused even when target exists.
    let escape_file = tmp2.path().join("escape_probe.txt");
    std::fs::write(&escape_file, b"outside secret\n").expect("write escape file");
    let dotdot_path = state2
        .join("staging")
        .join("..")
        .join("..")
        .join("escape_probe.txt");
    assert!(
        world2.read_file(&dotdot_path).is_err(),
        "`..` traversal in Disk::read_file must be refused"
    );
}

#[cfg(unix)]
#[test]
fn disk_read_file_refuses_concurrent_file_and_ancestor_symlink_swaps() {
    use fieldbook::{World as _, WorldError};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    let tmp = tempfile::tempdir().expect("create tempdir");
    let state_dir = tmp.path().join("state");
    let real_staging = state_dir.join("staging_real");
    let active_staging = state_dir.join("staging");
    let outside_dir = tmp.path().join("outside_secret_dir");
    std::fs::create_dir_all(real_staging.join("sub")).expect("create real_staging");
    std::fs::create_dir_all(outside_dir.join("sub")).expect("create outside_dir");

    let legit_bytes = b"legitimate staged page bytes\n";
    let secret_bytes = b"OUTSIDE SECRET BYTES MUST NEVER BE READ\n";
    std::fs::write(real_staging.join("sub/p0001.txt"), legit_bytes).expect("write legit page");
    std::fs::write(outside_dir.join("sub/p0001.txt"), secret_bytes).expect("write outside page");
    let outside_leaf = tmp.path().join("outside_leaf.txt");
    std::fs::write(&outside_leaf, secret_bytes).expect("write outside leaf");

    // Seed initial valid directory tree under `<state>/staging/sub/p0001.txt`.
    std::fs::create_dir_all(active_staging.join("sub")).expect("create active_staging");
    std::fs::write(active_staging.join("sub/p0001.txt"), legit_bytes).expect("seed active page");

    let stop = Arc::new(AtomicBool::new(false));
    let swaps = Arc::new(AtomicUsize::new(0));
    let active_clone = active_staging.clone();
    let real_clone = real_staging.clone();
    let outside_dir_clone = outside_dir.clone();
    let outside_leaf_clone = outside_leaf.clone();
    let stop_clone = Arc::clone(&stop);
    let swaps_clone = Arc::clone(&swaps);

    let swapper = std::thread::spawn(move || {
        let backup_dir = active_clone.with_file_name("staging_bak");
        let leaf_path = active_clone.join("sub/p0001.txt");
        let leaf_tmp = active_clone.join("sub/.swap_leaf");
        while !stop_clone.load(Ordering::Relaxed) {
            // 1. Atomic leaf file <-> symlink swap inside `sub/`.
            let _ = std::fs::remove_file(&leaf_tmp);
            if std::os::unix::fs::symlink(&outside_leaf_clone, &leaf_tmp).is_ok() {
                let _ = std::fs::rename(&leaf_tmp, &leaf_path);
                swaps_clone.fetch_add(1, Ordering::Relaxed);
            }
            let _ = std::fs::remove_file(&leaf_tmp);
            if std::fs::write(&leaf_tmp, legit_bytes).is_ok() {
                let _ = std::fs::rename(&leaf_tmp, &leaf_path);
            }

            // 2. Ancestor directory `<state>/staging` <-> outside symlink swap.
            let _ = std::fs::remove_dir_all(&backup_dir);
            if std::fs::rename(&active_clone, &backup_dir).is_ok() {
                let _ = std::os::unix::fs::symlink(&outside_dir_clone, &active_clone);
                swaps_clone.fetch_add(1, Ordering::Relaxed);
                let _ = std::fs::remove_file(&active_clone);
                let _ = std::fs::rename(&backup_dir, &active_clone);
            } else {
                // Ensure `active_clone` stays restored if interrupted mid-step.
                let _ = std::fs::remove_file(&active_clone);
                if !active_clone.exists() {
                    let _ = std::fs::create_dir_all(active_clone.join("sub"));
                    let _ = std::fs::copy(
                        real_clone.join("sub/p0001.txt"),
                        active_clone.join("sub/p0001.txt"),
                    );
                }
            }
        }
    });

    let world = support::world::TestWorld::with_state(tmp.path(), &state_dir);
    let target = active_staging.join("sub/p0001.txt");
    let mut refused = 0usize;
    let mut ok_reads = 0usize;
    let mut iter = 0usize;
    while iter < 1500 || swaps.load(Ordering::Relaxed) < 10 {
        iter += 1;
        match world.read_file(&target) {
            Ok(Some(bytes)) => {
                assert_ne!(
                    bytes.as_slice(),
                    secret_bytes,
                    "TOCTOU escape: Disk::read_file read bytes from outside `<state>`!"
                );
                assert_eq!(bytes.as_slice(), legit_bytes);
                ok_reads += 1;
            }
            Ok(None) => {}
            Err(WorldError::OutOfScope(_)) => {
                refused += 1;
            }
            Err(WorldError::Io { .. }) => {
                // Concurrent rename between directory open and child lookup can yield ENOTDIR.
                refused += 1;
            }
            Err(other) => panic!("unexpected WorldError variant: {other:?}"),
        }
        if iter >= 20_000 {
            break;
        }
    }

    stop.store(true, Ordering::Relaxed);
    swapper.join().expect("join swapper thread");
    assert!(
        swaps.load(Ordering::Relaxed) > 0,
        "swapper thread must perform concurrent swaps"
    );
    assert!(
        refused > 0 || ok_reads > 0,
        "concurrent reader must observe live reads/refusals"
    );
}

/// A leaf swapped between a regular file and a FIFO while `Disk::read_file` runs must never block
/// the reader: the leaf is opened with `O_NONBLOCK | O_NOFOLLOW` and rejected by `fstat` before any
/// byte is read. A blocking open waits forever on a FIFO with no writer, so the watchdog fails.
#[cfg(target_os = "linux")]
#[test]
fn disk_read_file_never_blocks_on_a_leaf_swapped_to_a_fifo() {
    use fieldbook::{World as _, WorldError};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;

    let tmp = tempfile::tempdir().expect("create tempdir");
    let state_dir = tmp.path().join("state");
    let sub = state_dir.join("staging/sub");
    std::fs::create_dir_all(&sub).expect("create staging/sub");
    let legit_bytes = b"legitimate staged page bytes\n";
    let leaf = sub.join("p0001.txt");
    let parked_file = sub.join(".parked_file");
    let parked_fifo = sub.join(".parked_fifo");
    std::fs::write(&leaf, legit_bytes).expect("write leaf");
    let made = std::process::Command::new("mkfifo")
        .arg(&parked_fifo)
        .status()
        .expect("run mkfifo");
    assert!(made.success(), "mkfifo must create the FIFO");

    let stop = Arc::new(AtomicBool::new(false));
    let swaps = Arc::new(AtomicUsize::new(0));
    let swapper = {
        let (stop, swaps) = (Arc::clone(&stop), Arc::clone(&swaps));
        let (leaf, parked_file, parked_fifo) =
            (leaf.clone(), parked_file.clone(), parked_fifo.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                // Regular file out, FIFO in; then back again. Renames keep both inodes.
                if std::fs::rename(&leaf, &parked_file).is_ok() {
                    if std::fs::rename(&parked_fifo, &leaf).is_ok() {
                        swaps.fetch_add(1, Ordering::Relaxed);
                        let _ = std::fs::rename(&leaf, &parked_fifo);
                    }
                    let _ = std::fs::rename(&parked_file, &leaf);
                }
            }
        })
    };

    let world = support::world::TestWorld::with_state(tmp.path(), &state_dir);
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let reader = {
        let swaps = Arc::clone(&swaps);
        let target = leaf.clone();
        std::thread::spawn(move || {
            let mut iter = 0usize;
            while iter < 100_000 || swaps.load(Ordering::Relaxed) < 200 {
                iter += 1;
                match world.read_file(&target) {
                    Ok(Some(bytes)) => assert_eq!(bytes.as_slice(), legit_bytes),
                    Ok(None) | Err(WorldError::Io { .. }) => {}
                    Err(other) => panic!("unexpected WorldError variant: {other:?}"),
                }
                if iter >= 500_000 {
                    break;
                }
            }
            let _ = done_tx.send(iter);
        })
    };

    let outcome = done_rx.recv_timeout(Duration::from_secs(60));
    stop.store(true, Ordering::Relaxed);
    swapper.join().expect("join swapper thread");
    let iters =
        outcome.expect("Disk::read_file blocked: a leaf swapped to a FIFO was opened blocking");
    reader.join().expect("join reader thread");
    assert!(iters > 0);
    assert!(
        swaps.load(Ordering::Relaxed) > 0,
        "the swapper must put the FIFO in place while the reader runs"
    );
}

/// Where no descriptor-relative open exists, `Disk::read_file` refuses instead of walking pathnames.
#[cfg(all(unix, not(target_os = "linux")))]
#[test]
fn disk_read_file_refuses_without_descriptor_relative_opens() {
    use fieldbook::World as _;

    let tmp = tempfile::tempdir().expect("create tempdir");
    let state_dir = tmp.path().join("state");
    std::fs::create_dir_all(state_dir.join("staging")).expect("create staging");
    let leaf = state_dir.join("staging/p0001.txt");
    std::fs::write(&leaf, b"page\n").expect("write leaf");
    let world = support::world::TestWorld::with_state(tmp.path(), &state_dir);
    assert!(world.read_file(&leaf).is_err());
}

/// Windows: `<state>/staging` swapped by rename between the real folder and a junction to an
/// outside folder must never yield the outside bytes, and a junction in place is refused. The
/// checks are bound to the held handles (final path of each handle, no delete sharing), not to
/// the pathname. Junctions need no privilege; the test is skipped where `mklink /J` fails.
#[cfg(windows)]
#[test]
fn disk_read_file_refuses_junction_swaps_by_handle() {
    use fieldbook::{World as _, WorldError};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let tmp = tempfile::tempdir().expect("create tempdir");
    let state_dir = tmp.path().join("state");
    let staging = state_dir.join("staging");
    let parked_real = state_dir.join("staging_real");
    let parked_link = state_dir.join("staging_link");
    let outside = tmp.path().join("outside");
    std::fs::create_dir_all(staging.join("sub")).expect("create staging/sub");
    std::fs::create_dir_all(outside.join("sub")).expect("create outside/sub");
    let legit_bytes = b"legitimate staged page bytes\n";
    let secret_bytes = b"OUTSIDE SECRET BYTES MUST NEVER BE READ\n";
    std::fs::write(staging.join("sub/p0001.txt"), legit_bytes).expect("write legit page");
    std::fs::write(outside.join("sub/p0001.txt"), secret_bytes).expect("write outside page");
    let made = std::process::Command::new("cmd.exe")
        .args(["/C", "mklink", "/J"])
        .arg(&parked_link)
        .arg(&outside)
        .status();
    if !made.is_ok_and(|s| s.success()) {
        println!("skipped: mklink /J is not available");
        return;
    }

    let world = support::world::TestWorld::with_state(tmp.path(), &state_dir);
    let target = staging.join("sub/p0001.txt");

    // A junction standing at `<state>/staging` is refused outright.
    std::fs::rename(&staging, &parked_real).expect("park the real staging folder");
    std::fs::rename(&parked_link, &staging).expect("put the junction in place");
    assert!(
        world.read_file(&target).is_err(),
        "a junction at `<state>/staging` must be refused"
    );
    std::fs::rename(&staging, &parked_link).expect("park the junction");
    std::fs::rename(&parked_real, &staging).expect("restore the real staging folder");

    let stop = Arc::new(AtomicBool::new(false));
    let swapper = {
        let stop = Arc::clone(&stop);
        let (staging, parked_real, parked_link) =
            (staging.clone(), parked_real.clone(), parked_link.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if std::fs::rename(&staging, &parked_real).is_ok() {
                    if std::fs::rename(&parked_link, &staging).is_ok() {
                        let _ = std::fs::rename(&staging, &parked_link);
                    }
                    let _ = std::fs::rename(&parked_real, &staging);
                }
            }
        })
    };

    for _ in 0..5_000 {
        match world.read_file(&target) {
            Ok(Some(bytes)) => {
                assert_ne!(
                    bytes.as_slice(),
                    secret_bytes,
                    "TOCTOU escape: Disk::read_file read bytes through a junction swap"
                );
                assert_eq!(bytes.as_slice(), legit_bytes);
            }
            Ok(None) | Err(WorldError::OutOfScope(_) | WorldError::Io { .. }) => {}
            Err(other) => panic!("unexpected WorldError variant: {other:?}"),
        }
    }
    stop.store(true, Ordering::Relaxed);
    swapper.join().expect("join swapper thread");
}

#[test]
fn random_and_mutated_bytes_never_panic() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let input = std::fs::read(repo_root.join("tests/fixtures/ports/extractor/staged@1/input.txt"))
        .expect("read input.txt");
    let orig_sha = sha256_hex(&input);

    let tmp = tempfile::tempdir().expect("create tempdir");
    let state_dir = copy_state_intake_into(tmp.path());
    let toml_path = state_dir
        .join("staging")
        .join(&orig_sha)
        .join("extraction.toml");
    let orig_toml = std::fs::read(&toml_path).expect("read extraction.toml");
    let world = support::world::TestWorld::with_state(tmp.path(), &state_dir);
    let staged = Staged::new(&world);

    let mut rng = rng::Rng::new(1);
    for idx in 0..200 {
        let len = match idx {
            0 => 0,
            1 => 2048,
            _ => usize::try_from(rng.next_u64() % 2049).expect("len fits usize"),
        };
        let bytes = rng.bytes(len);
        let _ = staged.extract(&bytes, "txt");
        let text = String::from_utf8_lossy(&bytes);
        std::fs::write(&toml_path, text.as_bytes()).expect("write random toml");
        let _ = staged.extract(&input, "txt");
    }
    std::fs::write(&toml_path, &orig_toml).expect("restore extraction.toml");

    for pos in 0..orig_toml.len() {
        let mut flipped = orig_toml.clone();
        flipped[pos] ^= 0x01;
        std::fs::write(&toml_path, &flipped).expect("write flipped");
        let _ = staged.extract(&input, "txt");

        let mut deleted = orig_toml.clone();
        deleted.remove(pos);
        std::fs::write(&toml_path, &deleted).expect("write deleted");
        let _ = staged.extract(&input, "txt");
    }
}

#[test]
fn contract_backup_folder_1() {
    assert_eq!(fieldbook::io::backup_folder::FolderTarget::ID, "folder@1");

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fix_dir = repo_root.join("tests/fixtures/ports/backup/folder@1");
    let input = std::fs::read(fix_dir.join("input.txt")).expect("read input.txt");
    let expect_toml =
        std::fs::read_to_string(fix_dir.join("expect.toml")).expect("read expect.toml");
    let expect: toml::Table = toml::from_str(&expect_toml).expect("parse expect.toml");
    let expected_sha = expect
        .get("sha256")
        .and_then(toml::Value::as_str)
        .expect("sha256");

    let actual_sha = Sha256::digest(&input);
    assert_eq!(actual_sha.to_string(), expected_sha);

    support::backup_suite::backup_target_contract(&FolderTargetFactory);
}

#[test]
fn contract_store_folder_1() {
    store_suite::store_suite(&|p| Disk::with_state(p, p.join(".fieldbook")));

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fix_dir = repo_root.join("tests/fixtures/ports/store/folder@1");
    let input = std::fs::read(fix_dir.join("input.txt")).expect("read input.txt");
    let expect_toml =
        std::fs::read_to_string(fix_dir.join("expect.toml")).expect("read expect.toml");
    let expect: toml::Table = toml::from_str(&expect_toml).expect("parse expect.toml");
    let expected_path = expect
        .get("path")
        .and_then(toml::Value::as_str)
        .expect("path");
    let expected_sha = expect
        .get("sha256")
        .and_then(toml::Value::as_str)
        .expect("sha256");

    let actual_sha = Sha256::digest(&input);
    assert_eq!(actual_sha.to_string(), expected_sha);

    let tmp = tempfile::tempdir().expect("create tempdir");
    std::fs::create_dir_all(tmp.path().join("corpus")).expect("create corpus");
    std::fs::create_dir_all(tmp.path().join(".fieldbook")).expect("create .fieldbook");
    let disk = Disk::with_state(tmp.path(), tmp.path().join(".fieldbook"));

    let written_sha =
        store_suite::write_store_file(&disk, expected_path, &input).expect("write_store_file");
    assert_eq!(written_sha.to_string(), expected_sha);
}

#[test]
fn contract_authz_policy_readers_1() {
    authz_suite::authz_suite(&make_policy_readers);

    // Replay fixture
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fix_dir = repo_root.join("tests/fixtures/ports/authz/policy-readers@1");
    let input_str = std::fs::read_to_string(fix_dir.join("input.toml")).expect("read input.toml");
    let expect_str =
        std::fs::read_to_string(fix_dir.join("expect.toml")).expect("read expect.toml");

    let input_table: toml::Table = toml::from_str(&input_str).expect("parse input.toml");
    let expect_table: toml::Table = toml::from_str(&expect_str).expect("parse expect.toml");

    let mut groups = authz_suite::ScriptedGroups::new();
    if let Some(grp_tbl) = input_table.get("groups").and_then(toml::Value::as_table) {
        for (k, v) in grp_tbl {
            let ans = match v {
                toml::Value::Boolean(b) => Some(*b),
                toml::Value::String(s) if s == "unreadable" => None,
                _ => panic!("unexpected group answer: {v:?}"),
            };
            groups = groups.with_answer(k, ans);
        }
    }

    let mut store_specs = Vec::new();
    if let Some(store_arr) = input_table.get("store").and_then(toml::Value::as_array) {
        for st in store_arr {
            let id_str = st
                .get("id")
                .and_then(toml::Value::as_str)
                .expect("store id");
            let store_id = match id_str {
                "corpus" => StoreId::Corpus,
                "overlay" => StoreId::Overlay,
                "working" => StoreId::Working,
                "state" => StoreId::State,
                other => panic!("unknown store id {other}"),
            };
            let readers = st.get("readers").and_then(toml::Value::as_str);
            let changers = st.get("changers").and_then(toml::Value::as_str);
            store_specs.push((store_id, readers, changers));
        }
    }

    let authz = make_policy_readers(&store_specs, &groups);
    let actor = sample_actor();

    for (store_name, acts_val) in expect_table {
        let store_id = match store_name.as_str() {
            "corpus" => StoreId::Corpus,
            "overlay" => StoreId::Overlay,
            "working" => StoreId::Working,
            "state" => StoreId::State,
            other => panic!("unknown store in expect: {other}"),
        };
        let acts_tbl = acts_val.as_table().expect("store acts table");
        let scope = Scope::for_store(store_id);

        for (act_name, expected_val) in acts_tbl {
            let act = match act_name.as_str() {
                "read" => Act::Read,
                "propose" => Act::Propose,
                "accept" => Act::Accept,
                "export" => Act::Export,
                "admin" => Act::Admin,
                other => panic!("unknown act {other}"),
            };
            let expected_str = expected_val.as_str().expect("expected str");
            let decision = authz.allow(&actor, act, &scope);
            let actual_str = match decision {
                AuthzDecision::Allow => "allow".to_string(),
                AuthzDecision::Deny(word) => format!("deny:{}", word.as_str()),
            };
            assert_eq!(
                actual_str, expected_str,
                "store {store_name} act {act_name} failed fixture expectation"
            );
        }
    }
}

#[test]
fn caps_streaming_is_false_for_built_adapters() {
    for row in ADAPTERS {
        if row.state == State::Built {
            assert!(
                !row.caps().streaming,
                "{}: built adapter must have caps().streaming == false",
                row.id
            );
        }
    }

    // Local helper that takes Caps: verify it fails on synthetic row with streaming == true
    fn check_streaming(state: State, caps: Caps) -> Result<(), &'static str> {
        if state == State::Built && caps.streaming {
            return Err("streaming must be false for built adapter");
        }
        Ok(())
    }

    let streaming_caps = Caps::new(false, false, false, true, 0);
    assert!(check_streaming(State::Built, streaming_caps).is_err());
}

fn heading_anchor(port: Port, id: &str) -> String {
    let raw = format!("{} {}", port.as_str(), id);
    raw.to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '-')
        .collect::<String>()
        .replace(' ', "-")
}

fn extract_backticked(line: &str) -> Option<&str> {
    let start = line.find('`')? + 1;
    let end = line[start..].find('`')? + start;
    Some(&line[start..end])
}

fn check_ports_md_tables(ports_md: &str, rows: &[Row]) -> Result<(), String> {
    let marker_begin = "<!-- generated:ports -->";
    let marker_end = "<!-- /generated:ports -->";
    let Some(start) = ports_md.find(marker_begin) else {
        return Err("marker <!-- generated:ports --> not found in ports.md".to_owned());
    };
    let Some(end) = ports_md.find(marker_end) else {
        return Err("marker <!-- /generated:ports --> not found in ports.md".to_owned());
    };
    let generated_section = &ports_md[start + marker_begin.len()..end];

    let mut table_ids = BTreeSet::new();

    for section in generated_section.split("\n## ") {
        let first_line = section.lines().next().unwrap_or("").trim();
        if first_line.to_ascii_lowercase().starts_with("templates") {
            continue;
        }
        for line in section.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('|')
                && trimmed.contains('`')
                && let Some(id) = extract_backticked(trimmed)
            {
                table_ids.insert(id.to_owned());
            }
        }
    }

    for row in rows {
        if !table_ids.contains(row.id) {
            return Err(format!(
                "registry id {} missing from ports.md generated tables",
                row.id
            ));
        }
    }

    for id in &table_ids {
        if !rows.iter().any(|r| r.id == id.as_str()) {
            return Err(format!(
                "ports.md generated tables contain unknown id {}",
                id
            ));
        }
    }

    Ok(())
}

fn extract_string_literal(s: &str) -> Option<&str> {
    let start = s.find('"')? + 1;
    let end = s[start..].find('"')? + start;
    Some(&s[start..end])
}

fn find_const_id(content: &str) -> Option<&str> {
    let idx = content.find("const ID:")?;
    let rest = &content[idx..];
    let start = rest.find('"')? + 1;
    let end = rest[start..].find('"')? + start;
    Some(&rest[start..end])
}

fn collect_rs_files(
    dir: &Path,
    krate_name: &str,
    out: &mut Vec<(PathBuf, String)>,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) != Some("tests") {
                collect_rs_files(&path, krate_name, out)?;
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs")
            && path.file_name().and_then(|n| n.to_str()) != Some("doubles.rs")
            && !path.components().any(|c| c.as_os_str() == "tests")
        {
            out.push((path, krate_name.to_owned()));
        }
    }
    Ok(())
}

fn scan_impls_in_file(
    file_path: &Path,
    crate_name: &str,
    content: &str,
    rows: &[Row],
) -> Result<(), String> {
    let mut search_idx = 0;
    while let Some(impl_pos) = content[search_idx..].find("impl") {
        let actual_impl_pos = search_idx + impl_pos;
        search_idx = actual_impl_pos + 4;

        // Check word boundary
        if actual_impl_pos > 0 {
            let prev = content.as_bytes()[actual_impl_pos - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' {
                continue;
            }
        }
        let after_impl = &content[search_idx..];
        let Some(first_char) = after_impl.chars().next() else {
            break;
        };
        if first_char.is_ascii_alphanumeric() || first_char == '_' {
            continue;
        }

        let Some(open_brace_rel) = after_impl.find('{') else {
            break;
        };
        let header = &after_impl[..open_brace_rel];
        if header.contains(';') {
            continue;
        }

        let Some(for_rel) = header.find(" for ") else {
            continue;
        };

        let trait_part = header[..for_rel].trim();
        let type_part = header[for_rel + 5..].trim();

        let trait_name = trait_part
            .split('<')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .last()
            .unwrap_or("")
            .rsplit("::")
            .next()
            .unwrap_or("");
        let type_name = type_part
            .split('<')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .next()
            .unwrap_or("");

        let port = match trait_name {
            "Extractor" => Port::Extractor,
            "Admit" => {
                if type_name == "BorrowedAdmit" {
                    continue;
                }
                Port::Admit
            }
            "Authz" => Port::Authz,
            "BackupTarget" => Port::Backup,
            "World" => {
                if type_name == "Disk" {
                    Port::Store
                } else {
                    continue;
                }
            }
            _ => continue,
        };

        // Extract impl body
        let body_start = search_idx + open_brace_rel + 1;
        let mut depth = 1;
        let mut body_end = body_start;
        for (i, b) in content[body_start..].bytes().enumerate() {
            if b == b'{' {
                depth += 1;
            } else if b == b'}' {
                depth -= 1;
                if depth == 0 {
                    body_end = body_start + i;
                    break;
                }
            }
        }
        let impl_body = &content[body_start..body_end];

        let id = if port == Port::Store {
            "folder@1".to_string()
        } else if let Some(fn_id_pos) = impl_body.find("fn id(") {
            let fn_rest = &impl_body[fn_id_pos..];
            let Some(fn_brace_rel) = fn_rest.find('{') else {
                return Err(format!("{}: fn id missing body", file_path.display()));
            };
            let fn_body_start = fn_id_pos + fn_brace_rel + 1;
            let mut fn_depth = 1;
            let mut fn_body_end = fn_body_start;
            for (i, b) in impl_body[fn_body_start..].bytes().enumerate() {
                if b == b'{' {
                    fn_depth += 1;
                } else if b == b'}' {
                    fn_depth -= 1;
                    if fn_depth == 0 {
                        fn_body_end = fn_body_start + i;
                        break;
                    }
                }
            }
            let fn_body = &impl_body[fn_body_start..fn_body_end];

            if let Some(lit) = extract_string_literal(fn_body) {
                lit.to_string()
            } else if fn_body.contains("PLAIN_ID") {
                "plain@1".to_string()
            } else if fn_body.contains("HTML_STRIP_ID") {
                "html-strip@1".to_string()
            } else if fn_body.contains("PDFTOTEXT_LAYOUT_ID") {
                "pdftotext-layout@1".to_string()
            } else if let Some(lit) = find_const_id(content) {
                lit.to_string()
            } else {
                return Err(format!(
                    "{}: unresolvable id in fn id() for impl {} for {}",
                    file_path.display(),
                    trait_name,
                    type_name
                ));
            }
        } else if let Some(lit) = find_const_id(content) {
            lit.to_string()
        } else {
            return Err(format!(
                "{}: unresolvable id for impl {} for {}",
                file_path.display(),
                trait_name,
                type_name
            ));
        };

        let row = rows.iter().find(|r| r.port == port && r.id == id);
        let Some(row) = row else {
            return Err(format!(
                "{}: impl for trait {} with id {} has no registry row",
                file_path.display(),
                trait_name,
                id
            ));
        };

        if row.krate != crate_name {
            return Err(format!(
                "{}: row crate {} does not match file crate {}",
                file_path.display(),
                row.krate,
                crate_name
            ));
        }
    }

    Ok(())
}

fn scan_sources(repo: &Path, rows: &[Row]) -> Result<(), String> {
    let mut files = Vec::new();
    let crates_dir = repo.join("crates");
    if crates_dir.is_dir()
        && let Ok(entries) = std::fs::read_dir(&crates_dir)
    {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let krate_name = entry.file_name().to_string_lossy().to_string();
                let src_dir = entry.path().join("src");
                if src_dir.is_dir() {
                    collect_rs_files(&src_dir, &krate_name, &mut files)?;
                }
            }
        }
    }

    let xtask_src = repo.join("xtask/src");
    if xtask_src.is_dir() {
        collect_rs_files(&xtask_src, "xtask", &mut files)?;
    }

    for (file_path, krate_name) in files {
        let content = std::fs::read_to_string(&file_path)
            .map_err(|e| format!("{}: {e}", file_path.display()))?;

        let content_to_scan = if let Some(idx) = content.find("#[cfg(test)]") {
            &content[..idx]
        } else {
            &content[..]
        };

        scan_impls_in_file(&file_path, &krate_name, content_to_scan, rows)?;
    }

    Ok(())
}

/// Pure registry drift verification function.
pub fn registry_drift(
    rows: &[Row],
    repo: &Path,
    ports_md: &str,
    known_keys: Option<&[&str]>,
) -> Result<(), String> {
    // 1. Scan source files for trait implementations
    scan_sources(repo, rows)?;

    // 2. Check generated tables in ports.md
    check_ports_md_tables(ports_md, rows)?;

    // 3. For each row, check contracts, fixtures, and documentation headings
    for row in rows {
        if let Some(keys) = known_keys {
            for key in row.settings_keys {
                if !keys.contains(key) {
                    return Err(format!("{}: unknown settings key {key}", row.id));
                }
            }
        }

        if row.state == State::Built {
            // Check contract test function exists
            let contract_fn = format!("fn {}(", row.contract_name());
            let test_file = if row.krate == "xtask" {
                repo.join("xtask/src/extract.rs")
            } else if row.port == Port::Admit && row.krate == "fieldbook-core" {
                repo.join("crates/fieldbook-core/tests/pipeline.rs")
            } else if row.port == Port::Classifier && row.krate == "fieldbook-core" {
                repo.join("crates/fieldbook-core/tests/contract_rules.rs")
            } else {
                repo.join("crates/fieldbook/tests/adapters.rs")
            };

            let test_content = std::fs::read_to_string(&test_file)
                .map_err(|e| format!("read {}: {e}", test_file.display()))?;
            if !test_content.contains(&contract_fn) {
                return Err(format!(
                    "row {}: contract test {} not found in {}",
                    row.id,
                    contract_fn,
                    test_file.display()
                ));
            }

            // Check fixture folder
            let fix_dir = repo.join(row.fixtures);
            if !fix_dir.is_dir() {
                return Err(format!(
                    "row {}: fixtures dir {} does not exist",
                    row.id, row.fixtures
                ));
            }
            if !fix_dir.join("expect.toml").is_file() {
                return Err(format!(
                    "row {}: missing expect.toml in {}",
                    row.id, row.fixtures
                ));
            }
            let entries = std::fs::read_dir(&fix_dir).map_err(|e| e.to_string())?;
            let has_input = entries
                .flatten()
                .any(|e| e.file_name().to_string_lossy().starts_with("input."));
            if !has_input {
                return Err(format!(
                    "row {}: missing input.* file in {}",
                    row.id, row.fixtures
                ));
            }

            // Check doc heading and anchor
            let expected_heading = format!("### {} {}", row.port.as_str(), row.id);
            if !ports_md.contains(&expected_heading) {
                return Err(format!(
                    "row {}: heading '{}' not found in ports.md",
                    row.id, expected_heading
                ));
            }
            let anchor = heading_anchor(row.port, row.id);
            if anchor != row.doc {
                return Err(format!(
                    "row {}: expected doc anchor {}, got {}",
                    row.id, anchor, row.doc
                ));
            }
        }
    }

    Ok(())
}

fn synthetic_ports_md(rows: &[Row]) -> String {
    let mut out = String::from("# Ports and Adapters\n\n<!-- generated:ports -->\n");
    for &port in &Port::ALL {
        let port_rows: Vec<&Row> = rows.iter().filter(|r| r.port == port).collect();
        if !port_rows.is_empty() {
            out.push_str(&format!("\n## {}\n\n", port.as_str()));
            out.push_str("| ID | Default | State |\n|---|---|---|\n");
            for r in port_rows {
                out.push_str(&format!(
                    "| `{}` | {} | {} |\n",
                    r.id,
                    r.default,
                    r.state.as_str()
                ));
            }
        }
    }
    out.push_str("\n<!-- /generated:ports -->\n\n");
    for r in rows {
        out.push_str(&format!(
            "### {} {}\n\nDocumentation text.\n\n",
            r.port.as_str(),
            r.id
        ));
    }
    out
}

#[test]
fn adapters_match_registry_docs_and_fixtures() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let ports_md =
        std::fs::read_to_string(repo_root.join("docs/reference/ports.md")).expect("read ports.md");
    registry_drift(ADAPTERS, &repo_root, &ports_md, None)
        .expect("adapters should match registry docs and fixtures");
}

#[test]
fn drift_passes_on_real_repo_with_synthetic_ports_md() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let ports_md = synthetic_ports_md(ADAPTERS);
    registry_drift(ADAPTERS, &repo_root, &ports_md, Some(&[]))
        .expect("drift check should pass on real repo with synthetic ports_md");
}

#[test]
fn drift_names_a_planted_adapter_file() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let planted_dir = tmp.path().join("crates/fieldbook/src/adapters");
    std::fs::create_dir_all(&planted_dir).expect("create planted dir");
    let planted_file = planted_dir.join("planted.rs");
    std::fs::write(
        &planted_file,
        b"pub struct Planted;\n\nimpl Extractor for Planted {\n    fn id(&self) -> &'static str {\n        \"planted@1\"\n    }\n}\n",
    )
    .expect("write planted.rs");

    let ports_md = synthetic_ports_md(ADAPTERS);
    let res = registry_drift(ADAPTERS, tmp.path(), &ports_md, None);
    assert!(res.is_err(), "planted adapter must be rejected");
    let err = res.unwrap_err();
    assert!(
        err.contains("planted.rs"),
        "error message must name planted.rs: {err}"
    );
}

#[test]
fn drift_names_a_planted_adapter_file_with_a_qualified_trait_path() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let planted_dir = tmp.path().join("crates/fieldbook/src/adapters");
    std::fs::create_dir_all(&planted_dir).expect("create planted dir");
    let planted_file = planted_dir.join("planted_qualified.rs");
    std::fs::write(
        &planted_file,
        b"pub struct Planted;\n\nimpl fieldbook_core::extraction::Extractor for Planted {\n    fn id(&self) -> &'static str {\n        \"planted@1\"\n    }\n}\n",
    )
    .expect("write planted_qualified.rs");

    let ports_md = synthetic_ports_md(ADAPTERS);
    let res = registry_drift(ADAPTERS, tmp.path(), &ports_md, None);
    assert!(
        res.is_err(),
        "a planted adapter with a qualified trait path must be rejected"
    );
    let err = res.unwrap_err();
    assert!(
        err.contains("planted_qualified.rs"),
        "error message must name planted_qualified.rs: {err}"
    );
}

#[test]
fn drift_fails_when_a_row_is_missing() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let missing_rows: Vec<Row> = ADAPTERS
        .iter()
        .copied()
        .filter(|r| r.id != "plain@1")
        .collect();
    let ports_md = synthetic_ports_md(&missing_rows);
    let res = registry_drift(&missing_rows, &repo_root, &ports_md, None);
    assert!(res.is_err(), "missing plain@1 row must fail drift check");
    let err = res.unwrap_err();
    assert!(
        err.contains("plain@1"),
        "error must name missing row plain@1: {err}"
    );
}

#[test]
fn drift_fails_on_unknown_settings_key() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut custom_rows = ADAPTERS.to_vec();
    custom_rows[0].settings_keys = &["unknown_key"];
    let ports_md = synthetic_ports_md(&custom_rows);
    let res = registry_drift(&custom_rows, &repo_root, &ports_md, Some(&["known_key"]));
    assert!(res.is_err(), "unknown settings key must fail drift check");
    let err = res.unwrap_err();
    assert!(
        err.contains("unknown_key"),
        "error must name unknown setting: {err}"
    );
}

#[test]
fn drift_fails_on_wrong_crate() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut custom_rows = ADAPTERS.to_vec();
    custom_rows[0].krate = "wrong-crate";
    let ports_md = synthetic_ports_md(&custom_rows);
    let res = registry_drift(&custom_rows, &repo_root, &ports_md, None);
    assert!(res.is_err(), "wrong crate must fail drift check");
    let err = res.unwrap_err();
    assert!(
        err.contains("wrong-crate"),
        "error must name wrong crate: {err}"
    );
}

/// Pure helper for verifying that extraction methods match registered extractors and documentation.
pub fn check_methods_match_extractors(
    methods: &[Method],
    rows: &[Row],
    repo: &Path,
    corpus_format_md: &str,
) -> Result<(), String> {
    let extraction_section = corpus_format_md
        .split("### Extraction")
        .nth(1)
        .ok_or_else(|| "### Extraction section not found in corpus-format.md".to_owned())?;
    let key_table = extraction_section
        .split("| Key | Meaning |")
        .nth(1)
        .ok_or_else(|| "Key | Meaning table not found in corpus-format.md".to_owned())?
        .split("\n\n")
        .next()
        .unwrap_or("");

    for m in methods {
        if m.name == "supplied" {
            continue;
        }
        let Some(extractor_id) = m.extractor else {
            return Err(format!("{}: extractor id is None", m.name));
        };
        let is_registered = rows
            .iter()
            .any(|r| r.port == Port::Extractor && r.id == extractor_id);
        if !is_registered {
            return Err(format!(
                "{}: extractor {} not found in ADAPTERS",
                m.name, extractor_id
            ));
        }

        let fix_path = repo.join(m.fixture);
        if !fix_path.exists() {
            return Err(format!("{}: fixture {} does not exist", m.name, m.fixture));
        }

        for &key in m.keys {
            let pat1 = format!("`{key}`");
            let pat2 = format!("`[{key}]`");
            if !key_table.contains(&pat1) && !key_table.contains(&pat2) {
                return Err(format!(
                    "{}: key {} not found in Key | Meaning table",
                    m.name, key
                ));
            }
        }
    }

    for r in rows {
        if r.port == Port::Extractor && r.id != "staged@1" {
            let is_named = methods.iter().any(|m| m.extractor == Some(r.id));
            if !is_named {
                return Err(format!(
                    "{}: registered Extractor row not named by any METHODS row",
                    r.id
                ));
            }
        }
    }

    Ok(())
}

#[test]
fn methods_match_extractors() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_format_md =
        std::fs::read_to_string(repo_root.join("docs/reference/corpus-format.md"))
            .expect("read corpus-format.md");

    check_methods_match_extractors(METHODS, ADAPTERS, &repo_root, &corpus_format_md)
        .expect("methods must match extractors");
}

#[test]
fn methods_fails_on_unregistered_extractor() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_format_md =
        std::fs::read_to_string(repo_root.join("docs/reference/corpus-format.md"))
            .expect("read corpus-format.md");

    let mut custom_methods = METHODS.to_vec();
    custom_methods[0].extractor = Some("unregistered@1");

    let res =
        check_methods_match_extractors(&custom_methods, ADAPTERS, &repo_root, &corpus_format_md);
    assert!(
        res.is_err(),
        "unregistered extractor must fail methods check"
    );
    let err = res.unwrap_err();
    assert!(
        err.contains("unregistered@1"),
        "error must name unregistered extractor: {err}"
    );
}

#[test]
fn methods_fails_on_unmatched_extractor_adapter() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_format_md =
        std::fs::read_to_string(repo_root.join("docs/reference/corpus-format.md"))
            .expect("read corpus-format.md");

    let mut custom_rows = ADAPTERS.to_vec();
    let staged_row = custom_rows
        .iter_mut()
        .find(|r| r.id == "staged@1")
        .expect("staged@1 row must exist in ADAPTERS");
    staged_row.id = "other-replay@1";

    let res = check_methods_match_extractors(METHODS, &custom_rows, &repo_root, &corpus_format_md);
    assert!(
        res.is_err(),
        "non-staged@1 Extractor row without producer METHODS entry must fail"
    );
    let err = res.unwrap_err();
    assert!(
        err.contains("other-replay@1"),
        "error must name unmatched extractor adapter: {err}"
    );
}
