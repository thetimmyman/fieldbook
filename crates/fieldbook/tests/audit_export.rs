//! Binary tests for `fieldbook audit export` and `fieldbook audit verify --export`
//! (`audit-export` steps 5 to 11).

#![cfg(feature = "test-hooks")]
#![expect(
    clippy::expect_used,
    clippy::disallowed_methods,
    reason = "integration tests copy fixtures, inspect export folders, and drive the real binary"
)]

mod support;

#[path = "support/policy.rs"]
mod policy_support;

use std::path::{Path, PathBuf};

use fieldbook::World;
use fieldbook::io::audit_export::{Layout, export, store_uid_of};
use fieldbook::io::journal::{AtomicStage, Journal, set_atomic_stage_hook};
use fieldbook::io::state;
use fieldbook::policy::PolicyCtx;
use fieldbook_core::audit::export::{Manifest, plan_export, verify_export};
use fieldbook_core::audit::{
    DestKind, EventPayload, Kind, LabelSet, Now, Sha256, Via, Word, ZERO_HASH,
    chain::{seal_body, split_hash_suffix},
};
use fieldbook_core::check::Code;
use fieldbook_core::labels::PlaceFacts;
use fieldbook_core::policy::{ClosedWhy, Policy, Seen};
use policy_support::write_policy;
use support::world::TestWorld;

const AUDIT_POLICY: &str = include_str!("../../../tests/fixtures/policy/audit.toml.in");
const GOLDEN_VERIFY_EXPORT: &str =
    include_str!("../../../tests/golden/json/audit-verify-export.json");
const STEPS_RS: &str = include_str!("../src/io/audit_export.rs");
const EXPORT_RS: &str = include_str!("../../fieldbook-core/src/audit/export.rs");

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn corpus_mini() -> PathBuf {
    repo_root().join("tests/fixtures/corpus-mini")
}

fn fixture_today() -> Now {
    let date = fieldbook_core::CivilDate::parse("2026-03-01").expect("fixture date");
    Now::new(date, 43_200).expect("fixture time")
}

struct Fx {
    tmp: tempfile::TempDir,
    state: PathBuf,
    sink: PathBuf,
    policy_path: PathBuf,
    policy_ctx: PolicyCtx,
}

/// A path as the inside of a TOML basic string (Windows paths hold backslashes).
fn toml_escape(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "\\\\")
}

fn fixture() -> Fx {
    let tmp = tempfile::tempdir().expect("temp dir");
    let state = tmp.path().join("state");
    support::copy_dir_recursive(&repo_root().join("tests/fixtures/state-mini"), &state)
        .expect("copy state-mini");
    let sink = tmp.path().join("sink");
    std::fs::create_dir_all(&sink).expect("create sink");
    let text = AUDIT_POLICY.replace("SINK", &toml_escape(&sink));
    let policy_path = write_policy(tmp.path(), "policy.toml", &text);
    let policy = Policy::parse(&text, None).expect("parse audit policy");
    let policy_ctx = PolicyCtx::loaded(policy);
    Fx {
        tmp,
        state,
        sink,
        policy_path,
        policy_ctx,
    }
}

fn sink_canon(fx: &Fx) -> PathBuf {
    // The program resolves paths without the Windows verbatim prefix; the facts are keyed by that.
    let canon = fx.sink.canonicalize().expect("canonical sink");
    let text = canon.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => canon,
    }
}

fn open<'a>(world: &'a TestWorld, fx: &Fx) -> Journal<'a> {
    state::open(world, &fx.state, Via::Cli).expect("open journal")
}

fn manifest_path(fx: &Fx, file: &str) -> PathBuf {
    fx.sink.join(file)
}

fn read_manifest(fx: &Fx, name: &str) -> Manifest {
    let text = std::fs::read_to_string(manifest_path(fx, name)).expect("read manifest");
    Manifest::parse(&text).expect("parse manifest")
}

fn planned_name(journal: &Journal<'_>, policy: &PolicyCtx) -> String {
    let lines = journal.lines().expect("journal lines");
    let chain = lines
        .first()
        .map(|line| line.hash.clone())
        .unwrap_or_else(Sha256::zero);
    let since = journal
        .last_export_to_seq()
        .map_or(1, |seq| seq.saturating_add(1));
    plan_export(
        &lines,
        chain,
        journal.writer(),
        &store_uid_of(journal),
        since,
        policy.hash.clone(),
        env!("CARGO_PKG_VERSION"),
    )
    .expect("plan")
    .file_name
}

fn no_sink_policy() -> Policy {
    Policy::parse("policy_schema = 1\nrevision = 1\n", None).expect("parse no-audit policy")
}

fn labelled_policy(sink: &Path) -> Policy {
    let text = format!(
        concat!(
            "policy_schema = 1\nrevision = 1\n",
            "[[label]]\nid = \"np-0007\"\n",
            "title = \"Synthetic NDA title canary\"\n",
            "banner = \"Synthetic NDA banner canary\"\n",
            "[audit]\nsink = {:?}\nmax_unshipped_hours = 24\n"
        ),
        sink.to_string_lossy()
    );
    Policy::parse(&text, None).expect("parse labelled policy")
}

fn events(fx: &Fx) -> Vec<fieldbook_core::audit::Event> {
    fieldbook::io::journal::read_valid_events_prefix(&fx.state)
}

fn audit_export_events(fx: &Fx) -> Vec<fieldbook_core::audit::Event> {
    events(fx)
        .into_iter()
        .filter(|event| matches!(event.payload, EventPayload::AuditExport { .. }))
        .collect()
}

#[test]
fn export_writes_the_lines_and_a_manifest_under_the_sink() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let mut journal = open(&world, &fx);
    let report = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect("export");
    assert!(report.written);
    assert!(report.events > 0);
    assert!(fx.sink.join(&report.file).is_file());
    assert!(fx.sink.join(&report.manifest).is_file());
    let count = std::fs::read_dir(&fx.sink).expect("read sink").count();
    assert_eq!(count, 2);
}

// control: AL-06
#[test]
fn the_exported_file_equals_the_journal_lines() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let mut journal = open(&world, &fx);
    let report = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect("export");
    let exported = std::fs::read(fx.sink.join(&report.file)).expect("read export");
    let live: String = journal
        .lines()
        .expect("journal lines")
        .iter()
        .take(report.events)
        .map(|line| line.raw.as_str())
        .collect();
    assert_eq!(exported, live.as_bytes());
}

#[test]
fn a_second_export_starts_after_the_last_exported_seq() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let mut journal = open(&world, &fx);
    let first = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect("first export");
    let second = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect("second export");
    assert_eq!(second.first_seq, first.last_seq + 1);
    assert_ne!(first.file, second.file);
}

#[test]
fn since_one_re_exports_everything_with_equal_bytes() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let mut journal = open(&world, &fx);
    let first = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect("first export");
    let first_bytes = std::fs::read(fx.sink.join(&first.file)).expect("read first");
    let again = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        Some(1),
    )
    .expect("re-export");
    assert_eq!(again.first_seq, 1);
    let again_bytes = std::fs::read(fx.sink.join(&again.file)).expect("read again");
    assert!(
        again_bytes.starts_with(&first_bytes),
        "the re-export must carry the first export's lines unchanged"
    );
}

// control: AL-06
#[test]
fn a_file_with_other_bytes_is_fb0806_and_nothing_is_written() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let mut journal = open(&world, &fx);
    let name = planned_name(&journal, &fx.policy_ctx);
    let path = fx.sink.join(&name);
    std::fs::write(&path, b"x").expect("plant conflicting bytes");
    let err = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect_err("conflict must be refused");
    assert_eq!(err.code, Code::Fb0806);
    assert_eq!(std::fs::read(&path).expect("read planted"), b"x");
    assert!(
        audit_export_events(&fx).is_empty(),
        "a refused export writes no audit.export event"
    );

    let approved = b"approved-bytes\n";
    let (allowed, audited, direct) =
        one_file_transaction(&world, &fx, &mut journal, "conflict-static.jsonl", approved);
    std::fs::write(&direct, b"x").expect("plant conflicting bytes");
    let err = fieldbook::io::write_export(&world, &allowed, &audited, &direct, approved)
        .expect_err("write_export must not overwrite other bytes");
    assert!(matches!(err, fieldbook::io::WriteExportError::Conflict));
    assert_eq!(std::fs::read(&direct).expect("read planted"), b"x");
    let _ = journal.end(audited, Word::new_static("failed"));
}

#[test]
fn a_target_outside_the_sink_is_refused() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let mut journal = open(&world, &fx);
    let err = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        Some(fx.tmp.path()),
        None,
    )
    .expect_err("outside sink must be refused");
    assert_eq!(err.code, Code::Fb0806);
    assert_eq!(err.reason, "no-row");
}

#[test]
fn a_sink_inside_the_data_root_is_refused() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let text = AUDIT_POLICY.replace("SINK", &toml_escape(&fx.state));
    let policy = Policy::parse(&text, None).expect("parse policy");
    let ctx = PolicyCtx::loaded(policy);
    let mut journal = open(&world, &fx);
    let err = export(&world, &Layout::of(&world), &ctx, &mut journal, None, None)
        .expect_err("sink inside the data root must be refused");
    assert_eq!(err.code, Code::Fb0806);
    assert_eq!(err.reason, "store");
}

#[test]
fn a_sink_in_a_sync_root_is_refused() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    world.set_place(
        &sink_canon(&fx),
        PlaceFacts {
            reparse: false,
            sync_root: true,
            network: false,
        },
    );
    let mut journal = open(&world, &fx);
    let err = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect_err("sync-root sink must be refused");
    assert_eq!(err.code, Code::Fb0806);
    assert_eq!(err.reason, "store");
}

#[test]
fn a_sink_on_a_network_share_is_valid() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    world.set_place(
        &sink_canon(&fx),
        PlaceFacts {
            reparse: false,
            sync_root: false,
            network: true,
        },
    );
    let mut journal = open(&world, &fx);
    let report = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect("network sink is valid");
    assert!(report.written);
}

#[test]
fn the_floor_has_no_sink_and_is_fb0806() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let args = vec![
        "audit".to_owned(),
        "export".to_owned(),
        "--state".to_owned(),
        fx.state.to_string_lossy().into_owned(),
    ];
    let outcome = fieldbook::run(&args, &world);
    assert_eq!(outcome.code, 1, "stderr: {}", outcome.stderr);
    assert!(
        outcome.stderr.contains("FB0806"),
        "stderr: {}",
        outcome.stderr
    );
    assert!(outcome.stderr.contains("no [audit] sink"));
}

#[test]
fn a_policy_with_no_audit_table_is_fb0806() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let ctx = PolicyCtx::loaded(no_sink_policy());
    let mut journal = open(&world, &fx);
    let err = export(&world, &Layout::of(&world), &ctx, &mut journal, None, None)
        .expect_err("no audit table must be refused");
    assert_eq!(err.code, Code::Fb0806);
    assert_eq!(err.reason, "no-row");
}

#[test]
fn a_closed_policy_exports_to_the_recorded_sink() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let mut ctx = PolicyCtx::closed(ClosedWhy::Invalid);
    ctx.seen_to_record = Some(Seen {
        revision: 1,
        hash: Sha256::digest(b"synthetic-policy"),
        audit_sink: Some(fx.sink.clone()),
    });
    let mut journal = open(&world, &fx);
    let report = export(&world, &Layout::of(&world), &ctx, &mut journal, None, None)
        .expect("closed export to the recorded sink");
    assert!(report.written);
}

#[test]
fn a_closed_policy_with_no_recorded_sink_is_fb0806() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let ctx = PolicyCtx::closed(ClosedWhy::Invalid);
    let mut journal = open(&world, &fx);
    let err = export(&world, &Layout::of(&world), &ctx, &mut journal, None, None)
        .expect_err("closed without a recorded sink must be refused");
    assert_eq!(err.code, Code::Fb0806);
}

#[test]
fn the_events_are_begin_then_files_then_end_then_audit_export() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let mut journal = open(&world, &fx);
    export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect("export");
    let tail: Vec<Kind> = events(&fx)
        .iter()
        .rev()
        .take(3)
        .rev()
        .map(|event| event.kind())
        .collect();
    assert_eq!(
        tail,
        vec![Kind::WriteBegin, Kind::WriteEnd, Kind::AuditExport]
    );
    let tail = events(&fx);
    let begin = tail
        .iter()
        .rev()
        .find(|event| event.kind() == Kind::WriteBegin)
        .expect("write.begin");
    match &begin.payload {
        EventPayload::WriteBegin {
            dest,
            labels,
            policy,
            ..
        } => {
            assert_eq!(*dest, Some(DestKind::Audit));
            assert_eq!(labels, &LabelSet::public());
            assert!(policy.is_some());
        }
        other => panic!("unexpected begin payload: {other:?}"),
    }
    let end = tail
        .iter()
        .rev()
        .find(|event| event.kind() == Kind::WriteEnd)
        .expect("write.end");
    match &end.payload {
        EventPayload::WriteEnd { outcome, dest, .. } => {
            assert_eq!(outcome.as_str(), "ok");
            assert_eq!(*dest, Some(DestKind::Audit));
        }
        other => panic!("unexpected end payload: {other:?}"),
    }
}

#[test]
fn audit_export_carries_range_head_and_policy() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let mut journal = open(&world, &fx);
    let report = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect("export");
    let found = audit_export_events(&fx);
    let last = found.last().expect("audit.export");
    match &last.payload {
        EventPayload::AuditExport {
            from_seq,
            to_seq,
            head,
            policy,
            ..
        } => {
            assert_eq!(from_seq.value(), report.first_seq);
            assert_eq!(to_seq.value(), report.last_seq);
            assert_eq!(head, &report.head);
            assert_eq!(policy, &fx.policy_ctx.hash);
        }
        other => panic!("unexpected payload: {other:?}"),
    }
}

#[test]
fn a_refusal_appends_a_refuse_event_with_dest_audit() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let ctx = PolicyCtx::loaded(no_sink_policy());
    let mut journal = open(&world, &fx);
    let _ =
        export(&world, &Layout::of(&world), &ctx, &mut journal, None, None).expect_err("refused");
    let tail = events(&fx);
    let last = tail.last().expect("a refusal event");
    match &last.payload {
        EventPayload::Refuse {
            code, dest, labels, ..
        } => {
            assert_eq!(code.id(), "FB0806");
            assert_eq!(*dest, Some(DestKind::Audit));
            assert_eq!(labels, &Some(LabelSet::public()));
        }
        other => panic!("unexpected payload: {other:?}"),
    }
}

#[test]
fn nothing_new_writes_nothing() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let mut journal = open(&world, &fx);
    let end = journal.lines().expect("lines").last().expect("a line").seq;
    let report = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        Some(end + 1),
    )
    .expect("nothing new");
    assert!(!report.written);
    assert_eq!(std::fs::read_dir(&fx.sink).expect("read sink").count(), 0);
    assert!(audit_export_events(&fx).is_empty());
}

#[test]
fn default_export_carries_the_previous_exports_own_events() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let mut journal = open(&world, &fx);
    let first = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect("first export");
    let second = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect("second export");
    let begin_line = journal
        .lines()
        .expect("lines")
        .into_iter()
        .find(|line| line.seq == first.last_seq + 1)
        .expect("the first export's write.begin");
    let second_bytes = std::fs::read(fx.sink.join(&second.file)).expect("read second");
    assert!(
        second_bytes.starts_with(begin_line.raw.as_bytes()),
        "the second default export must start at the first one's write.begin"
    );
}

#[test]
fn verify_export_needs_no_state_folder() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let mut journal = open(&world, &fx);
    let report = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect("export");
    let manifest = manifest_path(&fx, &report.manifest);
    let empty_state = fx.tmp.path().join("empty-state");
    std::fs::create_dir_all(&empty_state).expect("empty state");
    let bare = TestWorld::with_state(&corpus_mini(), &empty_state);
    let args = vec![
        "audit".to_owned(),
        "verify".to_owned(),
        "--export".to_owned(),
        manifest.to_string_lossy().into_owned(),
        "--state".to_owned(),
        empty_state.to_string_lossy().into_owned(),
    ];
    let outcome = fieldbook::run(&args, &bare);
    assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);
    assert!(
        outcome.stdout.starts_with("export ok:"),
        "{}",
        outcome.stdout
    );
}

#[test]
fn verify_export_json_matches_the_golden() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let mut journal = open(&world, &fx);
    let report = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect("export");
    let manifest = manifest_path(&fx, &report.manifest);
    let args = vec![
        "audit".to_owned(),
        "verify".to_owned(),
        "--export".to_owned(),
        manifest.to_string_lossy().into_owned(),
        "--json".to_owned(),
    ];
    let outcome = fieldbook::run(&args, &world);
    assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);
    assert_eq!(outcome.stdout, GOLDEN_VERIFY_EXPORT);
}

#[test]
fn export_starts_no_process_and_opens_no_socket() {
    for (name, source) in [
        ("io/audit_export.rs", STEPS_RS),
        ("audit/export.rs", EXPORT_RS),
    ] {
        for needle in [
            "std::process",
            "Command::new",
            "TcpStream",
            "TcpListener",
            "UdpSocket",
            "std::net",
        ] {
            assert!(
                !source.contains(needle),
                "{name} must not name `{needle}`: the export makes no connection and starts no process"
            );
        }
    }
}

#[test]
fn the_manifest_names_the_store_uid() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let mut journal = open(&world, &fx);
    let report = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect("export");
    let text = std::fs::read_to_string(fx.sink.join(&report.manifest)).expect("manifest");
    assert!(text.contains("store_uid = \"00000000-0000-0000-0000-000000000000\""));
}

#[test]
fn the_event_and_the_manifest_carry_the_same_store_uid() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let mut journal = open(&world, &fx);
    let report = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect("export");
    let manifest = read_manifest(&fx, &report.manifest);
    let event = audit_export_events(&fx).pop().expect("audit.export");
    match event.payload {
        EventPayload::AuditExport { store_uid, .. } => {
            assert_eq!(store_uid.as_str(), manifest.store_uid);
        }
        other => panic!("unexpected payload: {other:?}"),
    }
}

#[test]
fn a_failed_export_does_not_reset_the_age() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    world.set_now(Now::new(fixture_today().date, 3600).expect("time"));
    let mut journal = open(&world, &fx);
    export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect("first export");
    let exported_at = journal.last_time_of(Kind::AuditExport).expect("exported");
    world.set_now(Now::new(fixture_today().date, 7200).expect("time"));
    let name = planned_name(&journal, &fx.policy_ctx);
    std::fs::write(fx.sink.join(&name), b"x").expect("plant conflict");
    let _ = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect_err("refused");
    assert_eq!(journal.last_time_of(Kind::AuditExport), Some(exported_at));
}

#[test]
fn shipped_uses_the_newest_export() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let early = Now::new(fixture_today().date, 3600).expect("time");
    world.set_now(early);
    let mut journal = open(&world, &fx);
    export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect("first export");
    let late = Now::new(fixture_today().date, 7200).expect("time");
    world.set_now(late);
    export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect("second export");
    assert_eq!(journal.last_time_of(Kind::AuditExport), Some(late));
}

#[test]
fn no_title_or_banner_in_the_export() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let policy = labelled_policy(&fx.sink);
    let ctx = PolicyCtx::loaded(policy);
    let mut journal = open(&world, &fx);
    let report =
        export(&world, &Layout::of(&world), &ctx, &mut journal, None, None).expect("export");
    let text = std::fs::read_to_string(fx.sink.join(&report.file)).expect("read file");
    let manifest = std::fs::read_to_string(fx.sink.join(&report.manifest)).expect("read manifest");
    for canary in ["Synthetic NDA title canary", "Synthetic NDA banner canary"] {
        assert!(!text.contains(canary), "export holds `{canary}`");
        assert!(!manifest.contains(canary), "manifest holds `{canary}`");
    }
}

#[test]
fn audit_export_does_not_trigger_store_reminder() {
    let fx = fixture();
    std::fs::write(
        fx.state.join("settings.toml"),
        concat!(
            "role = \"lead\"\n",
            "[backup]\nwarn_after_days = 0\n",
            "[[backup.target]]\nid = \"t\"\nkind = \"folder\"\npath = \""
        )
        .to_owned()
            + &toml_escape(&fx.tmp.path().join("backups"))
            + "\"\n",
    )
    .expect("write settings");
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let args = vec![
        "audit".to_owned(),
        "export".to_owned(),
        "--state".to_owned(),
        fx.state.to_string_lossy().into_owned(),
        "--policy".to_owned(),
        fx.policy_path.to_string_lossy().into_owned(),
    ];
    let outcome = fieldbook::run(&args, &world);
    assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);
    assert!(
        !Journal::thread_wrote_store(),
        "an audit export writes outside the store and must not set wrote_store"
    );
    assert!(
        !outcome.stderr.to_ascii_lowercase().contains("backup"),
        "stderr: {}",
        outcome.stderr
    );
}

#[test]
fn audit_output_preserves_selected_lines_when_closed() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let mut ctx = PolicyCtx::closed(ClosedWhy::Invalid);
    ctx.seen_to_record = Some(Seen {
        revision: 1,
        hash: Sha256::digest(b"synthetic-policy"),
        audit_sink: Some(fx.sink.clone()),
    });
    let mut journal = open(&world, &fx);
    let report =
        export(&world, &Layout::of(&world), &ctx, &mut journal, None, None).expect("closed export");
    let exported = std::fs::read(fx.sink.join(&report.file)).expect("read");
    let live: String = journal
        .lines()
        .expect("lines")
        .iter()
        .take(report.events)
        .map(|line| line.raw.as_str())
        .collect();
    assert_eq!(exported, live.as_bytes());
}

#[test]
fn audit_output_binds_both_paths_and_hashes() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let mut journal = open(&world, &fx);
    let report = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect("export");
    let journal_path = fx.sink.join(&report.file);
    let manifest_path = fx.sink.join(&report.manifest);
    assert!(journal_path.is_file());
    assert!(manifest_path.is_file());
    let manifest = read_manifest(&fx, &report.manifest);
    assert_eq!(manifest.file, report.file);
    let exported = std::fs::read(&journal_path).expect("read export");
    assert_eq!(manifest.lines_sha256, Sha256::digest(&exported));
    let begin = events(&fx)
        .into_iter()
        .rev()
        .find(|event| event.kind() == Kind::WriteBegin)
        .expect("write.begin");
    match begin.payload {
        EventPayload::WriteBegin {
            files,
            dest,
            path_sha256,
            policy,
            ..
        } => {
            assert_eq!(files.value(), 2);
            assert_eq!(dest, Some(DestKind::Audit));
            assert!(path_sha256.is_some());
            assert!(policy.is_some());
        }
        other => panic!("unexpected payload: {other:?}"),
    }
}

#[test]
fn audit_export_binds_both_files() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let mut journal = open(&world, &fx);
    let report = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect("export");
    let manifest = read_manifest(&fx, &report.manifest);
    let exported = std::fs::read(fx.sink.join(&report.file)).expect("read export");
    assert_eq!(manifest.lines_sha256, Sha256::digest(&exported));
    assert!(
        verify_export(
            &std::fs::read_to_string(fx.sink.join(&report.manifest)).expect("manifest"),
            &exported,
            None,
            None
        )
        .is_empty()
    );
}

fn one_file_transaction<'a>(
    world: &'a TestWorld,
    fx: &Fx,
    journal: &mut Journal<'a>,
    name: &str,
    bytes: &[u8],
) -> (fieldbook_core::egress::Allowed, fieldbook::Audited, PathBuf) {
    let path = fx.sink.join(name);
    let delivery = fieldbook_core::egress::Delivery::single_write(&path, bytes, None);
    let effective = fx.policy_ctx.effective_policy(world);
    let allowed = match fieldbook_core::egress::check(
        &effective,
        &fieldbook_core::egress::Dest::Audit,
        Some(&LabelSet::public()),
        Some(&delivery),
        world.now(),
        fieldbook_core::egress::shipped(journal.last_time_of(Kind::AuditExport), world.now()),
    ) {
        fieldbook_core::egress::Decision::Allowed(allowed) => allowed,
        fieldbook_core::egress::Decision::Denied(denial) => panic!("denied: {denial}"),
    };
    let write_set = fieldbook_core::audit::WriteSet {
        labels: LabelSet::public(),
        files: vec![fieldbook_core::audit::WriteFile::new(
            name.to_owned(),
            Sha256::digest(bytes).as_str().to_owned(),
        )],
        affected: Vec::new(),
    };
    let audited = journal
        .begin_export("audit export", &write_set, &allowed, &sink_canon(fx))
        .expect("begin export");
    (allowed, audited, path)
}

#[test]
fn changed_export_file_is_refused() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let mut journal = open(&world, &fx);
    let approved = b"approved-bytes\n";
    let (allowed, audited, path) =
        one_file_transaction(&world, &fx, &mut journal, "approved.jsonl", approved);
    let err = fieldbook::io::write_export(&world, &allowed, &audited, &path, b"other-bytes\n")
        .expect_err("changed payload must be refused");
    assert!(matches!(err, fieldbook::io::WriteExportError::Io(_)));
    assert!(!path.exists());
    let rogue = fx.sink.join("rogue.jsonl");
    let err = fieldbook::io::write_export(&world, &allowed, &audited, &rogue, approved)
        .expect_err("changed target must be refused");
    assert!(matches!(err, fieldbook::io::WriteExportError::Io(_)));
    assert!(!rogue.exists());
    let _ = journal.end(audited, Word::new_static("failed"));
}

#[test]
fn file_created_after_export_preflight_is_not_overwritten() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let mut journal = open(&world, &fx);
    let planned = planned_name(&journal, &fx.policy_ctx);
    let manifest_name = planned.replace(".jsonl", ".manifest.toml");
    let manifest_path = fx.sink.join(&manifest_name);
    let journal_name = planned.clone();
    let manifest_for_hook = manifest_path.clone();
    set_atomic_stage_hook(Some(std::sync::Arc::new(move |stage, path| {
        if stage == AtomicStage::BeforeRename
            && path.file_name().and_then(|n| n.to_str()) == Some(journal_name.as_str())
        {
            let _ = std::fs::write(&manifest_for_hook, b"other-bytes\n");
        }
    })));
    let result = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    );
    set_atomic_stage_hook(None);
    let err = result.expect_err("a conflict in the race window must be refused");
    assert_eq!(err.code, Code::Fb0806);
    assert_eq!(err.reason, "mismatch");
    assert_eq!(
        std::fs::read(&manifest_path).expect("read conflicting file"),
        b"other-bytes\n"
    );
    assert!(audit_export_events(&fx).is_empty());
}

#[test]
fn export_anchor_matches_live_prefix() {
    let fx = fixture();
    let world = TestWorld::with_state(&corpus_mini(), &fx.state);
    let mut journal = open(&world, &fx);
    let report = export(
        &world,
        &Layout::of(&world),
        &fx.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect("export");
    let manifest = read_manifest(&fx, &report.manifest);
    let anchor = manifest.head.as_str().to_owned();
    let anchor_seq = manifest.last_seq;
    drop(journal);

    let anchor_args = |with_seq: bool| {
        let mut args = vec![
            "audit".to_owned(),
            "verify".to_owned(),
            "--state".to_owned(),
            fx.state.to_string_lossy().into_owned(),
        ];
        if with_seq {
            args.push("--anchor-seq".to_owned());
            args.push(anchor_seq.to_string());
        }
        args.push("--anchor-head".to_owned());
        args.push(anchor.clone());
        args
    };

    let outcome = fieldbook::run(&anchor_args(true), &world);
    assert_eq!(outcome.code, 0, "after export: {}", outcome.stderr);

    let appended = open(&world, &fx);
    drop(appended);
    let outcome = fieldbook::run(&anchor_args(true), &world);
    assert_eq!(outcome.code, 0, "after append: {}", outcome.stderr);

    let outcome = fieldbook::run(&anchor_args(false), &world);
    assert_eq!(outcome.code, 2, "omitting --anchor-seq is a usage error");

    reseal_prefix(&fx.state, anchor_seq);
    let outcome = fieldbook::run(&anchor_args(true), &world);
    assert_ne!(outcome.code, 0, "a rehashed prefix must fail the anchor");

    let fx2 = fixture();
    let world2 = TestWorld::with_state(&corpus_mini(), &fx2.state);
    let mut journal = open(&world2, &fx2);
    let report = export(
        &world2,
        &Layout::of(&world2),
        &fx2.policy_ctx,
        &mut journal,
        None,
        None,
    )
    .expect("export");
    let manifest = read_manifest(&fx2, &report.manifest);
    drop(journal);
    let _appended = open(&world2, &fx2);
    corrupt_tail(&fx2.state);
    let args = vec![
        "audit".to_owned(),
        "verify".to_owned(),
        "--state".to_owned(),
        fx2.state.to_string_lossy().into_owned(),
        "--anchor-seq".to_owned(),
        manifest.last_seq.to_string(),
        "--anchor-head".to_owned(),
        manifest.head.as_str().to_owned(),
    ];
    let outcome = fieldbook::run(&args, &world2);
    assert_ne!(outcome.code, 0, "a corrupt tail must fail verification");
}

fn reseal_prefix(state: &Path, upto: u64) {
    let path = state.join("audit/journal.jsonl");
    let text = std::fs::read_to_string(&path).expect("read journal");
    let mut out = String::new();
    let mut prev = ZERO_HASH.to_owned();
    for (index, line) in text.lines().enumerate() {
        let seq = u64::try_from(index).unwrap_or(0) + 1;
        if seq > upto {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let (body, _) = split_hash_suffix(line).expect("sealed line");
        let body = if seq == 1 {
            body.replacen("\"writer\":0", "\"writer\":1", 1)
        } else {
            body.to_owned()
        };
        let (sealed, hash) = seal_body(&prev, &body);
        out.push_str(&sealed);
        out.push('\n');
        prev = hash.as_str().to_owned();
    }
    std::fs::write(&path, out).expect("rewrite prefix");
}

fn corrupt_tail(state: &Path) {
    let path = state.join("audit/journal.jsonl");
    let text = std::fs::read_to_string(&path).expect("read journal");
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    if let Some(last) = lines.last_mut()
        && let Some(body) = last.find(",\"hash\":\"")
    {
        let mut bytes = last.clone().into_bytes();
        bytes[body + 9] = if bytes[body + 9] == b'a' { b'b' } else { b'a' };
        *last = String::from_utf8(bytes).expect("utf8");
    }
    std::fs::write(&path, format!("{}\n", lines.join("\n"))).expect("rewrite tail");
}
