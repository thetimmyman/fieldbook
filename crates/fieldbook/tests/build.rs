//! Integration tests for `fieldbook build` (`build-v0`).

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "integration tests spawn the fieldbook binary and inspect temporary directories and journal files"
)]

mod support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use fieldbook::build::{
    BuildPlanError, plan_edition, plan_edition_with_site, run_with_info, write_edition_outputs,
};
use fieldbook::cli::BuildArgs;
use fieldbook::io::journal::set_write_pause_hook;
use fieldbook::io::{dirs, state};
use fieldbook::{Error, World};
use fieldbook_core::CivilDate;
use fieldbook_core::audit::{DestKind, Event, EventPayload, Kind, LabelId, LabelSet, Via, Word};
use fieldbook_core::authz::ReadableStores;
use fieldbook_core::check::{Code, Host, Options, load};
use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
use fieldbook_core::labels::PUBLIC;
use fieldbook_core::policy::EffectivePolicy;
use fieldbook_core::strings;
use fieldbook_render::csp::CSP_STATIC;
use fieldbook_render::lint::{self, OriginalNames};
use fieldbook_render::meta::{BuildInfo, BuildProfile};
use fieldbook_render::site::{self, Site};

use support::temp_copy;
use support::world::TestWorld;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini")
}

fn run_bin(cwd: &Path, envs: &[(&str, &str)], args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fieldbook"));
    cmd.current_dir(cwd).args(args);
    for &(k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output()
        .unwrap_or_else(|e| panic!("failed to spawn fieldbook binary: {e}"))
}

fn collect_tree_bytes(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let entries =
            std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
        let mut paths: Vec<PathBuf> = entries
            .map(|res| res.unwrap_or_else(|e| panic!("dir entry: {e}")).path())
            .collect();
        paths.sort();
        for path in paths {
            let meta = std::fs::symlink_metadata(&path)
                .unwrap_or_else(|e| panic!("metadata {}: {e}", path.display()));
            if meta.is_dir() {
                walk(base, &path, out);
            } else if meta.is_file() {
                let rel = path
                    .strip_prefix(base)
                    .unwrap_or_else(|e| panic!("strip_prefix: {e}"))
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                let bytes =
                    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
                out.insert(rel, bytes);
            }
        }
    }

    let mut map = BTreeMap::new();
    if root.exists() {
        walk(root, root, &mut map);
    }
    map
}

fn read_journal_events(state_dir: &Path) -> Vec<Event> {
    let journal_path = state_dir.join("audit/journal.jsonl");
    if !journal_path.exists() {
        return Vec::new();
    }
    let raw = std::fs::read_to_string(&journal_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", journal_path.display()));
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            Event::parse_line(line)
                .unwrap_or_else(|e| panic!("parse journal line: {e}"))
                .0
        })
        .collect()
}

mod build {
    use super::*;

    #[test]
    fn refuses_a_corpus_with_errors() {
        let tmp = temp_copy(&fixture_root());
        std::fs::write(
            tmp.path().join("corpus/corpus.toml"),
            "schema_version = 999\n",
        )
        .unwrap_or_else(|e| panic!("corrupt corpus.toml: {e}"));

        let out_dir = tmp.path().join("out-err");
        let out_str = out_dir.to_string_lossy().into_owned();
        let output = run_bin(tmp.path(), &[], &["build", "--out", &out_str]);

        assert_eq!(
            output.status.code(),
            Some(1),
            "expected exit 1 on corpus errors, stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(Code::Fb0600.id()),
            "stderr must contain FB0600, got:\n{stderr}"
        );
        assert!(
            !out_dir.exists(),
            "nothing may be written when check reports errors"
        );
        assert!(
            read_journal_events(&tmp.path().join(".fieldbook")).is_empty(),
            "refused check must append no journal event"
        );
    }

    #[test]
    fn builds_the_fixture() {
        let tmp = temp_copy(&fixture_root());
        let output = run_bin(tmp.path(), &[], &["build"]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());

        let edition = tmp.path().join("out/edition");
        assert!(edition.join("index.html").is_file());
        assert!(edition.join("assets/strings.js").is_file());
        assert!(edition.join("assets/app.css").is_file());
        assert!(tmp.path().join("out/.fieldbook-out").is_file());

        let index_html = std::fs::read_to_string(edition.join("index.html"))
            .unwrap_or_else(|e| panic!("read index.html: {e}"));
        assert!(index_html.contains(CSP_STATIC));
        assert!(index_html.contains("<main></main>"));
    }

    #[test]
    fn build_out_is_the_parent_folder() {
        let tmp = temp_copy(&fixture_root());
        let parent = tmp.path().join("custom-out");
        let parent_str = parent.to_string_lossy().into_owned();
        let output = run_bin(tmp.path(), &[], &["build", "--out", &parent_str]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );

        assert!(
            parent.join("edition/index.html").is_file(),
            "$T/edition/index.html must exist"
        );
        assert!(
            !parent.join("index.html").exists(),
            "$T/index.html must not exist directly in parent folder"
        );
    }

    #[test]
    fn a_build_appends_one_write_begin_and_one_write_end() {
        let tmp = temp_copy(&fixture_root());
        let world = TestWorld::new(tmp.path());
        let state_dir = world.state_dir().to_path_buf();
        let saw_begin_before_write = Arc::new(AtomicBool::new(false));
        let hook_flag = Arc::clone(&saw_begin_before_write);
        let hook_state_dir = state_dir.clone();

        set_write_pause_hook(Some(Arc::new(move |_path| {
            let events = read_journal_events(&hook_state_dir);
            let write_begins = events
                .iter()
                .filter(|e| e.kind() == Kind::WriteBegin)
                .count();
            let write_ends = events.iter().filter(|e| e.kind() == Kind::WriteEnd).count();
            if write_begins == 1 && write_ends == 0 {
                hook_flag.store(true, Ordering::SeqCst);
            }
        })));

        let args = BuildArgs {
            out: PathBuf::from("out"),
        };
        let outcome = run_with_info(&args, &world, &BuildInfo::default());
        set_write_pause_hook(None);

        assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);
        assert!(
            saw_begin_before_write.load(Ordering::SeqCst),
            "Journal::begin_export must append write.begin before io::out::replace writes files"
        );

        let events = read_journal_events(&state_dir);
        let write_begins: Vec<&Event> = events
            .iter()
            .filter(|e| e.kind() == Kind::WriteBegin)
            .collect();
        let write_ends: Vec<&Event> = events
            .iter()
            .filter(|e| e.kind() == Kind::WriteEnd)
            .collect();
        assert_eq!(
            write_begins.len(),
            1,
            "expected exactly one write.begin, got {events:?}"
        );
        assert_eq!(
            write_ends.len(),
            1,
            "expected exactly one write.end, got {events:?}"
        );
        assert!(
            write_begins[0].seq < write_ends[0].seq,
            "write.begin must precede write.end"
        );

        match &write_begins[0].payload {
            EventPayload::WriteBegin {
                verb, labels, dest, ..
            } => {
                assert_eq!(verb.as_str(), "build");
                assert_eq!(labels, &LabelSet::public());
                assert_eq!(*dest, Some(DestKind::Edition));
            }
            other => panic!("expected WriteBegin payload, got {other:?}"),
        }
        match &write_ends[0].payload {
            EventPayload::WriteEnd { outcome, dest, .. } => {
                assert_eq!(outcome.as_str(), "ok");
                assert_eq!(*dest, Some(DestKind::Edition));
            }
            other => panic!("expected WriteEnd payload, got {other:?}"),
        }
    }

    // control: DT-01
    #[test]
    fn two_builds_are_identical() {
        let tmp = temp_copy(&fixture_root());
        let out_a = tmp.path().join("out-a");
        let out_b = tmp.path().join("out-b");
        let out_a_str = out_a.to_string_lossy().into_owned();
        let out_b_str = out_b.to_string_lossy().into_owned();

        let first = run_bin(
            tmp.path(),
            &[("TZ", "UTC"), ("SOURCE_DATE_EPOCH", "1772323200")],
            &["build", "--out", &out_a_str],
        );
        assert_eq!(
            first.status.code(),
            Some(0),
            "first build failed:\n{}",
            String::from_utf8_lossy(&first.stderr)
        );

        let subdir = tmp.path().join("corpus/pages");
        let second = run_bin(
            &subdir,
            &[
                ("TZ", "Pacific/Auckland"),
                ("SOURCE_DATE_EPOCH", "1772323200"),
            ],
            &["build", "--out", &out_b_str],
        );
        assert_eq!(
            second.status.code(),
            Some(0),
            "second build failed:\n{}",
            String::from_utf8_lossy(&second.stderr)
        );

        let files_a = collect_tree_bytes(&out_a.join("edition"));
        let files_b = collect_tree_bytes(&out_b.join("edition"));
        assert!(!files_a.is_empty(), "edition output must not be empty");

        let mut all_paths: Vec<String> = files_a.keys().chain(files_b.keys()).cloned().collect();
        all_paths.sort();
        all_paths.dedup();

        for rel in all_paths {
            let bytes_a = files_a.get(&rel);
            let bytes_b = files_b.get(&rel);
            if bytes_a != bytes_b {
                panic!("{}: two builds differed at `{rel}`", Code::Fb0606.id());
            }
        }
    }

    #[test]
    fn a_rebuild_drops_a_page_the_corpus_no_longer_has() {
        let tmp = temp_copy(&fixture_root());
        let out_t = tmp.path().join("out-rebuild");
        let out_t_str = out_t.to_string_lossy().into_owned();

        let first = run_bin(tmp.path(), &[], &["build", "--out", &out_t_str]);
        assert_eq!(first.status.code(), Some(0));

        let gone = out_t.join("edition/p/gone.html");
        std::fs::create_dir_all(gone.parent().unwrap_or_else(|| Path::new(".")))
            .unwrap_or_else(|e| panic!("mkdir p: {e}"));
        std::fs::write(&gone, b"<html>gone</html>\n")
            .unwrap_or_else(|e| panic!("write gone.html: {e}"));
        assert!(gone.is_file());

        let second = run_bin(tmp.path(), &[], &["build", "--out", &out_t_str]);
        assert_eq!(second.status.code(), Some(0));
        assert!(
            !gone.exists(),
            "rebuild must remove planted edition/p/gone.html"
        );

        let out_fresh = tmp.path().join("out-fresh");
        let out_fresh_str = out_fresh.to_string_lossy().into_owned();
        let fresh = run_bin(tmp.path(), &[], &["build", "--out", &out_fresh_str]);
        assert_eq!(fresh.status.code(), Some(0));

        assert_eq!(
            collect_tree_bytes(&out_t.join("edition")),
            collect_tree_bytes(&out_fresh.join("edition")),
            "rebuilt edition folder must equal a fresh build"
        );
    }

    #[test]
    fn a_foreign_edition_folder_is_not_deleted() {
        let tmp = temp_copy(&fixture_root());
        let foreign_out = tmp.path().join("foreign-out");
        let keep_file = foreign_out.join("edition/keep.txt");
        std::fs::create_dir_all(
            keep_file
                .parent()
                .unwrap_or_else(|| panic!("keep.txt parent")),
        )
        .unwrap_or_else(|e| panic!("create foreign edition dir: {e}"));
        std::fs::write(&keep_file, b"keep me\n").unwrap_or_else(|e| panic!("write keep.txt: {e}"));

        let foreign_str = foreign_out.to_string_lossy().into_owned();
        let output = run_bin(tmp.path(), &[], &["build", "--out", &foreign_str]);
        assert_eq!(
            output.status.code(),
            Some(2),
            "foreign root must exit 2, stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(
                "it was not made by fieldbook build; remove it yourself or choose another --out"
            ),
            "unexpected stderr: {stderr}"
        );
        assert_eq!(
            std::fs::read(&keep_file).unwrap_or_else(|e| panic!("read keep.txt: {e}")),
            b"keep me\n",
            "keep.txt in foreign edition folder must remain untouched"
        );
        assert!(
            read_journal_events(&tmp.path().join(".fieldbook")).is_empty(),
            "foreign root refusal must append no journal event"
        );
    }

    #[test]
    fn a_second_build_removes_a_stale_file() {
        let tmp = temp_copy(&fixture_root());
        let out_dir = tmp.path().join("out-stale");
        let out_str = out_dir.to_string_lossy().into_owned();

        let first = run_bin(tmp.path(), &[], &["build", "--out", &out_str]);
        assert_eq!(first.status.code(), Some(0));

        let stale = out_dir.join("edition/stale.txt");
        std::fs::write(&stale, b"stale\n").unwrap_or_else(|e| panic!("write stale.txt: {e}"));
        assert!(stale.is_file());

        let second = run_bin(tmp.path(), &[], &["build", "--out", &out_str]);
        assert_eq!(second.status.code(), Some(0));
        assert!(!stale.exists(), "second build must remove stale.txt");
        assert!(out_dir.join("edition/index.html").is_file());
    }

    #[test]
    fn build_binds_every_output() {
        let tmp = temp_copy(&fixture_root());
        let world = TestWorld::new(tmp.path());
        let out_dir = tmp.path().join("out-bound");

        let tree = world.read_tree().unwrap_or_else(|e| panic!("{e}"));
        let (model, report) = load(&Host::new(tree, world.today()), &Options::new(false));
        assert!(report.is_ok());

        let eff = EffectivePolicy::floor(false);
        let now = world.now();
        let shipped = Shipped::Hours(1);
        let Decision::Allowed(sel) = check(&eff, &Dest::Edition, None, None, now, shipped) else {
            panic!("expected selection Allowed");
        };
        let view = LabelledView::new(&model, &sel, &ReadableStores::corpus());

        let plan = plan_edition(
            &view,
            &BuildInfo::default(),
            &out_dir,
            &eff,
            &LabelSet::public(),
            now,
            shipped,
        )
        .unwrap_or_else(|e| panic!("plan_edition: {e:?}"))
        .unwrap_or_else(|d| panic!("plan_edition denied: {d:?}"));

        let mut journal =
            state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
        let audited = journal
            .begin_export("build", &plan.write_set, &plan.allowed, &plan.out_dir)
            .unwrap_or_else(|e| panic!("{e}"));
        write_edition_outputs(&plan, &audited).unwrap_or_else(|e| panic!("{e}"));
        journal
            .end(audited, Word::new_static("ok"))
            .unwrap_or_else(|e| panic!("{e}"));

        // Verify marker and every planned file exist on disk with exact bound bytes.
        assert_eq!(
            std::fs::read(&plan.marker_path).unwrap_or_else(|e| panic!("{e}")),
            dirs::marker_bytes()
        );
        for (rel, expected) in plan.site.files() {
            let disk = std::fs::read(plan.edition_dir.join(rel)).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(&disk, expected, "output `{rel}` bytes must match plan");
        }

        // Verify that every single output file and the marker is checked at the final sink
        // against the delivery-bound Allowed token.
        let file_keys: Vec<String> = plan.site.files().keys().cloned().collect();
        for target_rel in &file_keys {
            let probe_out = tmp
                .path()
                .join(format!("probe-{}", target_rel.replace(['/', '.'], "-")));
            let mut probe_plan = plan_edition(
                &view,
                &BuildInfo::default(),
                &probe_out,
                &eff,
                &LabelSet::public(),
                now,
                shipped,
            )
            .unwrap_or_else(|e| panic!("{e:?}"))
            .unwrap_or_else(|d| panic!("{d:?}"));

            let probe_audited = journal
                .begin_export(
                    "build",
                    &probe_plan.write_set,
                    &probe_plan.allowed,
                    &probe_plan.out_dir,
                )
                .unwrap_or_else(|e| panic!("{e}"));

            let mut mutated_site = Site::new();
            for (rel, bytes) in probe_plan.site.files() {
                if rel == target_rel {
                    let mut altered = bytes.clone();
                    altered.push(b'\n');
                    mutated_site.insert(rel, altered);
                } else {
                    mutated_site.insert(rel, bytes.clone());
                }
            }
            probe_plan.site = mutated_site;

            let res = write_edition_outputs(&probe_plan, &probe_audited);
            let _ = journal.end(probe_audited, Word::new_static("failed"));
            assert!(
                matches!(res, Err(Error::OutOfScope(_))),
                "final sink must refuse altered bytes for `{target_rel}`, got {res:?}"
            );
        }

        // Also probe the marker bytes at the final sink.
        let marker_probe_out = tmp.path().join("probe-marker");
        let mut marker_plan = plan_edition(
            &view,
            &BuildInfo::default(),
            &marker_probe_out,
            &eff,
            &LabelSet::public(),
            now,
            shipped,
        )
        .unwrap_or_else(|e| panic!("{e:?}"))
        .unwrap_or_else(|d| panic!("{d:?}"));
        let marker_audited = journal
            .begin_export(
                "build",
                &marker_plan.write_set,
                &marker_plan.allowed,
                &marker_plan.out_dir,
            )
            .unwrap_or_else(|e| panic!("{e}"));
        marker_plan.marker_bytes.push(b'!');
        let marker_res = write_edition_outputs(&marker_plan, &marker_audited);
        let _ = journal.end(marker_audited, Word::new_static("failed"));
        assert!(
            matches!(marker_res, Err(Error::OutOfScope(_))),
            "final sink must refuse altered marker bytes, got {marker_res:?}"
        );
    }

    #[test]
    fn changed_build_bytes_or_target_are_refused() {
        let tmp = temp_copy(&fixture_root());
        let world = TestWorld::new(tmp.path());
        let tree = world.read_tree().unwrap_or_else(|e| panic!("{e}"));
        let (model, report) = load(&Host::new(tree, world.today()), &Options::new(false));
        assert!(report.is_ok());

        let eff = EffectivePolicy::floor(false);
        let now = world.now();
        let shipped = Shipped::Hours(1);
        let Decision::Allowed(sel) = check(&eff, &Dest::Edition, None, None, now, shipped) else {
            panic!("expected selection Allowed");
        };
        let view = LabelledView::new(&model, &sel, &ReadableStores::corpus());
        let mut journal =
            state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));

        // 1. Byte mutation case at the final sink seam (after valid Allowed + Audited).
        let byte_out = tmp.path().join("sink-byte-case");
        let mut byte_plan = plan_edition(
            &view,
            &BuildInfo::default(),
            &byte_out,
            &eff,
            &LabelSet::public(),
            now,
            shipped,
        )
        .unwrap_or_else(|e| panic!("{e:?}"))
        .unwrap_or_else(|d| panic!("{d:?}"));

        let byte_audited = journal
            .begin_export(
                "build",
                &byte_plan.write_set,
                &byte_plan.allowed,
                &byte_plan.out_dir,
            )
            .unwrap_or_else(|e| panic!("{e}"));

        let mut altered_site = Site::new();
        for (rel, bytes) in byte_plan.site.files() {
            if rel == "index.html" {
                let mut mutated = bytes.clone();
                mutated.extend_from_slice(b"<!-- mutated after approval -->");
                altered_site.insert(rel, mutated);
            } else {
                altered_site.insert(rel, bytes.clone());
            }
        }
        byte_plan.site = altered_site;

        let byte_res = write_edition_outputs(&byte_plan, &byte_audited);
        let _ = journal.end(byte_audited, Word::new_static("failed"));
        assert!(
            matches!(byte_res, Err(Error::OutOfScope(_))),
            "altered build bytes after approval must be refused at the final sink, got {byte_res:?}"
        );
        assert!(
            !byte_plan.edition_dir.join("index.html").exists(),
            "refused byte-altered write must not create index.html"
        );

        // 2. Target path mutation case at the final sink seam (after valid Allowed + Audited).
        let path_out = tmp.path().join("sink-path-case");
        let mut path_plan = plan_edition(
            &view,
            &BuildInfo::default(),
            &path_out,
            &eff,
            &LabelSet::public(),
            now,
            shipped,
        )
        .unwrap_or_else(|e| panic!("{e:?}"))
        .unwrap_or_else(|d| panic!("{d:?}"));

        let path_audited = journal
            .begin_export(
                "build",
                &path_plan.write_set,
                &path_plan.allowed,
                &path_plan.out_dir,
            )
            .unwrap_or_else(|e| panic!("{e}"));

        let mut redirected_site = Site::new();
        for (rel, bytes) in path_plan.site.files() {
            if rel == "index.html" {
                redirected_site.insert("unapproved.html", bytes.clone());
            } else {
                redirected_site.insert(rel, bytes.clone());
            }
        }
        path_plan.site = redirected_site;

        let path_res = write_edition_outputs(&path_plan, &path_audited);
        let _ = journal.end(path_audited, Word::new_static("failed"));
        assert!(
            matches!(path_res, Err(Error::OutOfScope(_))),
            "altered target path after approval must be refused at the final sink, got {path_res:?}"
        );
        assert!(
            !path_plan.edition_dir.join("unapproved.html").exists(),
            "refused path-altered write must not create unapproved.html"
        );
    }

    #[test]
    fn public_shell_delivery_succeeds() {
        let tmp = temp_copy(&fixture_root());
        let world = TestWorld::new(tmp.path());
        let tree = world.read_tree().unwrap_or_else(|e| panic!("{e}"));
        let (model, report) = load(&Host::new(tree, world.today()), &Options::new(false));
        assert!(report.is_ok());

        let eff = EffectivePolicy::floor(false);
        let now = world.now();
        let shipped = Shipped::Hours(1);
        let Decision::Allowed(sel) = check(&eff, &Dest::Edition, None, None, now, shipped) else {
            panic!("expected selection Allowed");
        };
        let view = LabelledView::new(&model, &sel, &ReadableStores::corpus());

        // Unauthorized delivery (non-public label set) refuses before creating the destination.
        let unauth_out = tmp.path().join("unauth-out");
        let np = LabelId::parse("np-0007").unwrap_or_else(|e| panic!("{e}"));
        let non_public = LabelSet::from_ids([PUBLIC, np]);
        let unauth_res = plan_edition(
            &view,
            &BuildInfo::default(),
            &unauth_out,
            &eff,
            &non_public,
            now,
            shipped,
        )
        .unwrap_or_else(|e| panic!("{e:?}"));
        assert!(
            unauth_res.is_err(),
            "non-public label set must be denied for Dest::Edition"
        );
        assert!(
            !unauth_out.exists(),
            "unauthorized delivery must refuse before creating the destination folder"
        );

        // Authorized public shell delivery succeeds and writes the shell, marker, and assets.
        let ok_out = tmp.path().join("public-out");
        let outcome = run_with_info(
            &BuildArgs {
                out: ok_out.clone(),
            },
            &world,
            &BuildInfo::default(),
        );
        assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);

        assert_eq!(
            std::fs::read(ok_out.join(dirs::MARKER_FILE)).unwrap_or_else(|e| panic!("{e}")),
            dirs::marker_bytes()
        );
        let index = std::fs::read_to_string(ok_out.join("edition/index.html"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(index.contains(CSP_STATIC));
        assert!(index.contains("href=\"assets/app.css\""));
        assert!(index.contains("src=\"assets/strings.js\""));
        assert!(index.contains(strings::unreleased_build()));
        assert_eq!(
            std::fs::read_to_string(ok_out.join("edition/assets/strings.js"))
                .unwrap_or_else(|e| panic!("{e}")),
            strings::js()
        );
        assert!(ok_out.join("edition/assets/app.css").is_file());
    }

    #[test]
    fn released_build_without_heading_preserves_output() {
        let tmp = temp_copy(&fixture_root());
        let out_dir = tmp.path().join("out-preserve");

        // Replace `corpus/changes.md` with a valid file that has no `## edition-NNN` heading.
        std::fs::write(
            tmp.path().join("corpus/changes.md"),
            "# Changes\n\nNo edition heading yet.\n",
        )
        .unwrap_or_else(|e| panic!("write changes.md: {e}"));

        let world = TestWorld::new(tmp.path());

        // 1. Incomplete release metadata (date without commit) still yields the unreleased shell
        //    and populates existing owned output in `out_dir`.
        let built_date =
            CivilDate::from_ymd(2026, 3, 1).unwrap_or_else(|e| panic!("valid date: {e}"));
        let incomplete_info = BuildInfo {
            built: Some(built_date),
            commit: None,
            workbook_epoch: 1_772_323_200,
            profile: BuildProfile::Silent,
        };
        let initial = run_with_info(
            &BuildArgs {
                out: out_dir.clone(),
            },
            &world,
            &incomplete_info,
        );
        assert_eq!(
            initial.code, 0,
            "incomplete release metadata must still succeed with unreleased shell: {}",
            initial.stderr
        );
        let initial_index = std::fs::read_to_string(out_dir.join("edition/index.html"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(initial_index.contains(strings::unreleased_build()));

        // Plant a known byte snapshot in the owned `out_dir/edition/index.html` so we can verify
        // that a subsequent failed released build preserves every existing output byte.
        let preserved_marker = b"<!-- owned output before failed released build -->\n";
        std::fs::write(out_dir.join("edition/index.html"), preserved_marker)
            .unwrap_or_else(|e| panic!("{e}"));
        let before_files = collect_tree_bytes(&out_dir);
        let events_before = read_journal_events(world.state_dir());

        // 2. Now run with valid release date AND valid commit, while `changes.md` lacks an
        //    edition heading. Must return exit 1, preserve every output byte, and emit no write.begin.
        let released_info = BuildInfo {
            built: Some(built_date),
            commit: Some("0123456789abcdef0123456789abcdef01234567".to_owned()),
            workbook_epoch: 1_772_323_200,
            profile: BuildProfile::Silent,
        };
        let failed = run_with_info(
            &BuildArgs {
                out: out_dir.clone(),
            },
            &world,
            &released_info,
        );
        assert_eq!(
            failed.code, 1,
            "released build without edition heading must return exit 1, stderr: {}",
            failed.stderr
        );

        let after_files = collect_tree_bytes(&out_dir);
        assert_eq!(
            after_files, before_files,
            "failed released build must preserve every existing output byte"
        );

        let events_after = read_journal_events(world.state_dir());
        let write_begins_before = events_before
            .iter()
            .filter(|e| e.kind() == Kind::WriteBegin)
            .count();
        let write_begins_after = events_after
            .iter()
            .filter(|e| e.kind() == Kind::WriteBegin)
            .count();
        assert_eq!(
            write_begins_after, write_begins_before,
            "failed released build must emit no write.begin"
        );
    }

    #[test]
    fn the_fixture_site_is_lint_clean() {
        let tmp = temp_copy(&fixture_root());
        let world = TestWorld::new(tmp.path());
        let tree = world.read_tree().unwrap_or_else(|e| panic!("{e}"));
        let (model, report) = load(&Host::new(tree, world.today()), &Options::new(false));
        assert!(report.is_ok());

        let eff = EffectivePolicy::floor(false);
        let sel = match check(
            &eff,
            &Dest::Edition,
            None,
            None,
            world.now(),
            Shipped::Never,
        ) {
            Decision::Allowed(a) => a,
            Decision::Denied(d) => panic!("denied: {d:?}"),
        };
        let view = LabelledView::new(&model, &sel, &ReadableStores::corpus());
        let site = site::render(&view, &BuildInfo::default()).unwrap_or_else(|e| panic!("{e}"));
        let diags = lint::lint(&site, &OriginalNames::default());
        assert!(
            diags.is_empty(),
            "fixture site must be lint-clean, got:\n{}",
            fieldbook_core::check::Report::from_parts(diags, Vec::new()).render_text()
        );
    }

    #[test]
    fn a_lint_finding_writes_nothing() {
        let tmp = temp_copy(&fixture_root());
        let world = TestWorld::new(tmp.path());
        let tree = world.read_tree().unwrap_or_else(|e| panic!("{e}"));
        let (model, report) = load(&Host::new(tree, world.today()), &Options::new(false));
        assert!(report.is_ok());

        let eff = EffectivePolicy::floor(false);
        let sel = match check(
            &eff,
            &Dest::Edition,
            None,
            None,
            world.now(),
            Shipped::Never,
        ) {
            Decision::Allowed(a) => a,
            Decision::Denied(d) => panic!("denied: {d:?}"),
        };
        let view = LabelledView::new(&model, &sel, &ReadableStores::corpus());
        let ctx =
            site::render_context(&view, &BuildInfo::default()).unwrap_or_else(|e| panic!("{e}"));
        let mut site = site::render_with_context(&ctx);
        site.insert(
            "bad.html",
            format!(
                "<!doctype html><html><head><meta http-equiv=\"Content-Security-Policy\" content=\"{CSP_STATIC}\"></head><body><a href=\"missing.html\">Dead</a></body></html>"
            )
            .into_bytes(),
        );

        let out_dir = tmp.path().join("out-lint-fail");
        let res = plan_edition_with_site(
            &view,
            ctx,
            site,
            &out_dir,
            &eff,
            &LabelSet::public(),
            world.now(),
            Shipped::Never,
        );
        match res {
            Err(BuildPlanError::Lint(diags)) => {
                assert_eq!(diags.len(), 1);
                assert_eq!(diags[0].code, Code::Fb0603);
            }
            other => panic!("expected BuildPlanError::Lint, got {other:?}"),
        }
        assert!(
            !out_dir.exists(),
            "a lint finding must write no file to out_dir"
        );
        let events = read_journal_events(world.state_dir());
        assert!(
            events.iter().all(|e| e.kind() != Kind::WriteBegin),
            "a lint finding must append no write.begin"
        );
    }
}
